.PHONY: lint lint-ruff lint-pyright lint-hotreload lint-mypackage lint-external ci-lint ci-lint-ruff ci-lint-pyright ci-lint-hotreload ci-lint-mypackage ci-lint-external test-hotreload build-develop lint-rust ci-lint-rust

# Default target
all: lint

# Package directories
ROOT_DIR := ./firehot/
MYPACKAGE_DIR := ./mypackage/mypackage/
EXTERNAL_DIR := ./mypackage/external-package/
PKG_DIRS := $(ROOT_DIR) $(MYPACKAGE_DIR) $(EXTERNAL_DIR)
RUST_DIR := ./src/

# Define a function to run pyright on a specific directory
# Usage: $(call run_pyright,<directory>)
define run_pyright
	@echo "\n=== Running pyright on $(1) ==="; \
	(cd $(1) && echo '{"include": ["."], "exclude": [".."], "ignore": ["../"]}' > temp_pyright_config.json && \
	uv run pyright --project temp_pyright_config.json && \
	rm temp_pyright_config.json) || { echo "FAILED: pyright in $(1)"; exit 1; }; \
	echo "=== pyright completed successfully for $(1) ===";
endef

# Define a function to run ruff on a specific directory 
# Usage: $(call run_ruff,<directory>)
define run_ruff
	@echo "\n=== Running ruff on $(1) ==="; \
	echo "Running ruff format in $(1)"; \
	(cd $(1) && uv run ruff format .) || { echo "FAILED: ruff format in $(1)"; exit 1; }; \
	echo "Running ruff check --fix in $(1)"; \
	(cd $(1) && uv run ruff check --fix .) || { echo "FAILED: ruff check in $(1)"; exit 1; }; \
	echo "=== ruff completed successfully for $(1) ===";
endef

# Define a function to run rustfmt on Rust code
# Usage: $(call run_rustfmt,<directory>)
define run_rustfmt
	@echo "\n=== Running rustfmt on $(1) ==="; \
	(cd $(1) && cargo fmt) || { echo "FAILED: rustfmt in $(1)"; exit 1; }; \
	(cd $(1) && cargo fix --allow-dirty --allow-staged) || { echo "FAILED: rustfix in $(1)"; exit 1; }; \
	echo "=== rustfmt completed successfully for $(1) ===";
endef

# Define a function to run clippy on Rust code
# Usage: $(call run_clippy,<directory>)
define run_clippy
	@echo "\n=== Running clippy on $(1) ==="; \
	(cd $(1) && cargo clippy -- -D warnings) || { echo "FAILED: clippy in $(1)"; exit 1; }; \
	echo "=== clippy completed successfully for $(1) ===";
endef

# Define a function to run all Rust linting tools
# Usage: $(call lint_rust,<directory>)
define lint_rust
	@echo "\n=== Running all Rust linters on $(1) ===";
	$(call run_rustfmt,$(1))
	$(call run_clippy,$(1))
	@echo "=== All Rust linters completed successfully for $(1) ===";
endef

# Define a function to run all lint tools on a specific directory
# Usage: $(call lint_directory,<directory>)
define lint_directory
	@echo "\n=== Running all linters on $(1) ===";
	$(call run_ruff,$(1))
	$(call run_pyright,$(1))
	@echo "=== All linters completed successfully for $(1) ===";
endef

# Main lint target that runs all linting tools on all packages
lint: lint-hotreload lint-mypackage lint-external

# Package-specific lint targets
lint-hotreload:
	@echo "=== Linting hotreload package ==="
	$(call lint_directory,$(ROOT_DIR))
	$(call lint_rust,$(RUST_DIR))

lint-mypackage:
	@echo "=== Linting mypackage package ==="
	$(call lint_directory,$(MYPACKAGE_DIR))

lint-external:
	@echo "=== Linting external package ==="
	$(call lint_directory,$(EXTERNAL_DIR))

# Tool-specific targets that run across all packages
lint-ruff:
	@echo "=== Running ruff on all packages ==="
	@for dir in $(PKG_DIRS); do \
		$(call run_ruff,$$dir); \
	done
	@echo "\n=== Ruff linting completed successfully for all packages ==="

lint-pyright:
	@echo "=== Running pyright on all packages ==="
	@for dir in $(PKG_DIRS); do \
		$(call run_pyright,$$dir); \
	done
	@echo "\n=== Pyright type checking completed successfully for all packages ==="

# Rust-specific lint targets
lint-rust:
	@echo "=== Linting Rust code ==="
	$(call lint_rust,$(RUST_DIR))

# Define a function to run rustfmt in CI mode (check only)
# Usage: $(call run_rustfmt_ci,<directory>)
define run_rustfmt_ci
	@echo "\n=== Running rustfmt (check only) on $(1) ==="; \
	(cd $(1) && cargo fmt --check) || { echo "FAILED: rustfmt check in $(1)"; exit 1; }; \
	echo "=== rustfmt check completed successfully for $(1) ===";
endef

# Define a function to run all Rust linting tools in CI mode
# Usage: $(call lint_rust_ci,<directory>)
define lint_rust_ci
	@echo "\n=== Running all Rust linters (check only) on $(1) ===";
	$(call run_rustfmt_ci,$(1))
	$(call run_clippy,$(1))
	@echo "=== All Rust linters completed successfully for $(1) ===";
endef

# Define a function to run ruff in CI mode (check only, no fixes)
# Usage: $(call run_ruff_ci,<directory>)
define run_ruff_ci
	@echo "\n=== Running ruff (validation only) on $(1) ==="; \
	echo "Running ruff format --check in $(1)"; \
	(cd $(1) && uv run ruff format --check .) || { echo "FAILED: ruff format in $(1)"; exit 1; }; \
	echo "Running ruff check (no fix) in $(1)"; \
	(cd $(1) && uv run ruff check .) || { echo "FAILED: ruff check in $(1)"; exit 1; }; \
	echo "=== ruff validation completed successfully for $(1) ===";
endef

# Define a function to run all lint tools in CI mode on a specific directory
# Usage: $(call lint_directory_ci,<directory>)
define lint_directory_ci
	@echo "\n=== Running all linters (validation only) on $(1) ===";
	$(call run_ruff_ci,$(1))
	$(call run_pyright,$(1))
	@echo "=== All linters completed successfully for $(1) ===";
endef

# CI lint target that runs all linting tools on all packages (no fixes)
ci-lint: ci-lint-hotreload ci-lint-mypackage ci-lint-external

# Package-specific CI lint targets (no fixes)
ci-lint-hotreload:
	@echo "=== CI Linting hotreload package (validation only) ==="
	$(call lint_directory_ci,$(ROOT_DIR))
	$(call lint_rust_ci,$(RUST_DIR))

ci-lint-mypackage:
	@echo "=== CI Linting mypackage package (validation only) ==="
	$(call lint_directory_ci,$(MYPACKAGE_DIR))

ci-lint-external:
	@echo "=== CI Linting external package (validation only) ==="
	$(call lint_directory_ci,$(EXTERNAL_DIR))

# Tool-specific CI targets that run across all packages (no fixes)
ci-lint-ruff:
	@echo "=== Running ruff (validation only) on all packages ==="
	@for dir in $(PKG_DIRS); do \
		$(call run_ruff_ci,$$dir); \
	done
	@echo "\n=== Ruff validation completed successfully for all packages ==="

ci-lint-pyright:
	@echo "=== Running pyright on all packages ==="
	@for dir in $(PKG_DIRS); do \
		$(call run_pyright,$$dir); \
	done
	@echo "\n=== Pyright type checking completed successfully for all packages ==="

# Rust-specific CI lint target
ci-lint-rust:
	@echo "=== CI Linting Rust code (check only) ==="
	$(call lint_rust_ci,$(RUST_DIR))

# Test target for hotreload package
test-hotreload:
	@echo "=== Running tests for hotreload package ==="
	(cd $(ROOT_DIR) && uv run pytest -vvv) || { echo "FAILED: tests in $(ROOT_DIR)"; exit 1; }
	@echo "=== Tests completed successfully for hotreload package ==="

# Development build target
build-develop:
	@echo "=== Building development version for mypackage ==="
	cd mypackage && \
	(cd .. && uv run maturin build $(MATURIN_ARGS)) && \
	rm -f uv.lock && \
	uv sync
	@echo "=== Development build completed successfully ==="

# Show help
help:
	@echo "Available targets:"
	@echo " "
	@echo "  lint            - Run all linters on all packages (with fixes)"
	@echo "  lint-hotreload  - Run all linters on the root package only (with fixes)"
	@echo "  lint-mypackage  - Run all linters on mypackage only (with fixes)"
	@echo "  lint-external   - Run all linters on external-package only (with fixes)"
	@echo "  lint-ruff       - Run ruff linter only (all packages, with fixes)"
	@echo "  lint-pyright    - Run pyright type checker only (all packages)"
	@echo "  lint-rust       - Run Rust linters (rustfmt and clippy) on src directory"
	@echo " "
	@echo "  ci-lint         - Run all linters on all packages (validation only, no fixes)"
	@echo "  ci-lint-hotreload - Run all linters on the root package only (validation only)"
	@echo "  ci-lint-mypackage - Run all linters on mypackage only (validation only)"
	@echo "  ci-lint-external - Run all linters on external-package only (validation only)"
	@echo "  ci-lint-ruff    - Run ruff linter only (all packages, validation only)"
	@echo "  ci-lint-pyright - Run pyright type checker only (all packages)"
	@echo "  ci-lint-rust    - Run Rust linters in check-only mode on src directory"
	@echo " "
	@echo "  test-hotreload  - Run tests for the hotreload package"
	@echo "  build-develop   - Build development version in mypackage directory"
