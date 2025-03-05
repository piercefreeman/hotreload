use anyhow::{anyhow, Result};
use std::{
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use walkdir::WalkDir;

use rustpython_parser::{parse, Mode};
use rustpython_parser::ast::{
    Mod, Stmt,
    StmtIf, StmtWhile, StmtFunctionDef, StmtAsyncFunctionDef, StmtClassDef,
};

/// A simple structure to hold information about an import.
#[derive(Debug)]
struct ImportInfo {
    /// For an `import X`, this is "X". For a `from X import Y`, this is "X".
    module: String,
    /// The names imported from that module.
    names: Vec<String>,
    /// Whether this is a relative import (starts with . or ..)
    is_relative: bool,
}

/// Recursively traverse AST statements to collect import information.
/// Absolute (level == 0) imports are considered third-party.
fn collect_imports(stmts: &[Stmt]) -> Vec<ImportInfo> {
    let mut imports = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Import(import_stmt) => {
                for alias in &import_stmt.names {
                    imports.push(ImportInfo {
                        module: alias.name.to_string(),
                        names: vec![alias
                            .asname
                            .clone()
                            .unwrap_or_else(|| alias.name.clone())
                            .to_string()],
                        is_relative: false,
                    });
                }
            }
            Stmt::ImportFrom(import_from) => {
                if let Some(module_name) = &import_from.module {
                    let imported = import_from
                        .names
                        .iter()
                        .map(|alias| {
                            alias
                                .asname
                                .clone()
                                .unwrap_or_else(|| alias.name.clone())
                                .to_string()
                        })
                        .collect();
                    imports.push(ImportInfo {
                        module: module_name.to_string(),
                        names: imported,
                        is_relative: import_from.level.map_or(false, |level| level.to_u32() > 0),
                    });
                }
            }
            Stmt::If(inner) => {
                let if_stmt: &StmtIf = &*inner;
                imports.extend(collect_imports(&if_stmt.body));
                imports.extend(collect_imports(&if_stmt.orelse));
            }
            Stmt::While(inner) => {
                let while_stmt: &StmtWhile = &*inner;
                imports.extend(collect_imports(&while_stmt.body));
                imports.extend(collect_imports(&while_stmt.orelse));
            }
            Stmt::FunctionDef(inner) => {
                let func_def: &StmtFunctionDef = &*inner;
                imports.extend(collect_imports(&func_def.body));
            }
            Stmt::AsyncFunctionDef(inner) => {
                let func_def: &StmtAsyncFunctionDef = &*inner;
                imports.extend(collect_imports(&func_def.body));
            }
            Stmt::ClassDef(inner) => {
                let class_def: &StmtClassDef = &*inner;
                imports.extend(collect_imports(&class_def.body));
            }
            _ => {}
        }
    }
    imports
}

/// Detect the current package name by looking for setup.py, pyproject.toml, or top-level __init__.py files
fn detect_package_name(path: &Path) -> Option<String> {
    // Try to find setup.py
    for entry in WalkDir::new(path)
        .max_depth(2) // Only check top-level and immediate subdirectories
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        if file_path.file_name().unwrap_or_default() == "setup.py" {
            if let Ok(content) = fs::read_to_string(file_path) {
                // Look for name='package_name' or name="package_name"
                let name_re = regex::Regex::new(r#"name=["']([^"']+)["']"#).unwrap();
                if let Some(captures) = name_re.captures(&content) {
                    return Some(captures.get(1).unwrap().as_str().to_string());
                }
            }
        } else if file_path.file_name().unwrap_or_default() == "pyproject.toml" {
            if let Ok(content) = fs::read_to_string(file_path) {
                // Look for name = "package_name" in [project] or [tool.poetry] section
                let name_re = regex::Regex::new(r#"(?:\[project\]|\[tool\.poetry\]).*?name\s*=\s*["']([^"']+)["']"#).unwrap();
                if let Some(captures) = name_re.captures(&content) {
                    return Some(captures.get(1).unwrap().as_str().to_string());
                }
            }
        }
    }
    
    // If no setup.py or pyproject.toml found, use directory name as fallback
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
}

/// Given a path, scan for all Python files, parse them and extract the set of
/// absolute (non-relative) modules that are imported.
fn process_py_files(path: &Path) -> Result<(HashSet<String>, Option<String>)> {
    let mut third_party_modules = HashSet::new();
    let package_name = detect_package_name(path);
    
    println!("Detected package name: {:?}", package_name);

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().map(|ext| ext == "py").unwrap_or(false)
        })
    {
        let file_path = entry.path();
        let source = fs::read_to_string(file_path)?;
        let parsed = parse(&source, Mode::Module, file_path.to_string_lossy().as_ref())
            .map_err(|e| anyhow!("Failed to parse {}: {:?}", file_path.display(), e))?;
        // Extract statements from the module. Note: `Mod::Module` now has named fields.
        let stmts: &[Stmt] = match &parsed {
            Mod::Module(module) => &module.body,
            _ => {
                return Err(anyhow!(
                    "Unexpected AST format for module in file {}",
                    file_path.display()
                ))
            }
        };
        let imports = collect_imports(stmts);
        for imp in imports {
            // Skip relative imports and imports of the current package
            if !imp.is_relative && 
               !package_name.as_ref().map_or(false, |pkg| imp.module.starts_with(pkg)) {
                third_party_modules.insert(imp.module);
            }
        }
    }
    Ok((third_party_modules, package_name))
}

/// Spawn a Python process that imports the given modules and then waits for commands on stdin.
/// The Python process prints "IMPORTS_LOADED" to stdout once all imports are complete.
/// After that, it will listen for commands on stdin, which can include fork requests and code to execute.
fn spawn_python_loader(modules: &HashSet<String>) -> Result<Child> {
    // Create import code for Python to execute
    let mut import_lines = String::new();
    for module in modules {
        import_lines.push_str(&format!(
            "try:\n    __import__('{}')\nexcept ImportError as e:\n    print('Failed to import {}:', e)\n",
            module, module
        ));
    }
    
    // Path to the Python loader script
    let loader_script_path = Path::new("python/loader.py");
    if !loader_script_path.exists() {
        return Err(anyhow!("Python loader script not found at: {:?}", loader_script_path));
    }
    
    // Launch the Python process with the loader script
    let child = Command::new("python")
        .arg(loader_script_path)
        .arg(import_lines)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn python process: {}", e))?;
    
    Ok(child)
}

/// Main function tying all steps together.
pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("Usage: {} <path_to_scan>", args[0]));
    }
    let scan_path = PathBuf::from(&args[1]);

    // 1. Process Python files.
    let (modules, package_name) = process_py_files(&scan_path)?;
    println!("Package name: {:?}", package_name);
    println!("Found third-party modules to load: {:?}", modules);

    // 2. Spawn a Python process to load these modules.
    let mut child = spawn_python_loader(&modules)?;

    // 3. Read stdout until we see "IMPORTS_LOADED".
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow!("Failed to capture stdout from python process"))?;
    let mut stdin = child.stdin.take()
        .ok_or_else(|| anyhow!("Failed to capture stdin for python process"))?;
    
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    
    // Wait for imports to complete
    loop {
        if let Some(Ok(line)) = lines.next() {
            println!("Python process: {}", line);
            if line.trim() == "IMPORTS_LOADED" {
                break;
            }
        } else {
            return Err(anyhow!("Python process terminated unexpectedly before imports completed"));
        }
    }

    // 4. Demonstrate forking and executing code
    println!("Sending code to first child process...");
    writeln!(stdin, "FORK:print('Hello from child process 1')")
        .map_err(|e| anyhow!("Failed to write to stdin: {}", e))?;
    
    // Wait for response from the fork
    let mut fork_complete = false;
    while !fork_complete {
        if let Some(Ok(line)) = lines.next() {
            println!("Python process: {}", line);
            if line.starts_with("FORKED:") {
                // Successfully forked
                let fork_pid = line.trim_start_matches("FORKED:");
                println!("Forked child process with PID: {}", fork_pid);
            } else if line.starts_with("FORK_COMPLETE:") {
                fork_complete = true;
            } else if line.starts_with("FORK_ERROR:") {
                return Err(anyhow!("Error in child process: {}", 
                    line.trim_start_matches("FORK_ERROR:")));
            }
        } else {
            return Err(anyhow!("Python process terminated unexpectedly during first fork"));
        }
    }
    
    // Sleep briefly to ensure first process completes
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    // 5. Launch a second child process
    println!("Sending code to second child process...");
    writeln!(stdin, "FORK:print('Hello from child process 2')")
        .map_err(|e| anyhow!("Failed to write to stdin: {}", e))?;
    
    // Wait for response from the second fork
    fork_complete = false;
    while !fork_complete {
        if let Some(Ok(line)) = lines.next() {
            println!("Python process: {}", line);
            if line.starts_with("FORKED:") {
                // Successfully forked
                let fork_pid = line.trim_start_matches("FORKED:");
                println!("Forked second child process with PID: {}", fork_pid);
            } else if line.starts_with("FORK_COMPLETE:") {
                fork_complete = true;
            } else if line.starts_with("FORK_ERROR:") {
                return Err(anyhow!("Error in second child process: {}", 
                    line.trim_start_matches("FORK_ERROR:")));
            }
        } else {
            return Err(anyhow!("Python process terminated unexpectedly during second fork"));
        }
    }
    
    // 6. Clean up
    println!("Demo complete. Terminating Python process.");
    writeln!(stdin, "EXIT")
        .map_err(|e| anyhow!("Failed to write exit command: {}", e))?;
    
    child.wait()?;
    Ok(())
}
