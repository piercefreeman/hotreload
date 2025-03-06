"""
Child entrypoint for hotreload.

Intended for embeddable usage in Rust, can only import stdlib modules.

"""

import base64
import importlib
import importlib.util
import pickle
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from hotreload.embedded.types import SerializedCall

    module_path = "path"
    pickled_str = "pickled_str"

# These will imported dynamically by rust
module_path: str
pickled_str: str

# Decode base64 and unpickle
pickled_bytes = base64.b64decode(pickled_str)
data: "SerializedCall" = pickle.loads(pickled_bytes)

# If we have a module path, import it first to ensure the function is available
# Technically we could just unpickle the data and pickle will automatically try to resolve the module, but
# this lets us more explicitly handle errors and issue debugging logs.
module_path = data["func_module_path"]
if module_path:
    print(f"Importing module: {module_path}")
    # Try to import the module or reload it if already imported
    if module_path in sys.modules:
        importlib.reload(sys.modules[module_path])
    else:
        importlib.import_module(module_path)
else:
    raise Exception("No module path provided")

# Resolve the function from the module
func = getattr(sys.modules[module_path], data["func_name"])
args = data["args"]

# Run the function with args
if isinstance(args, tuple):
    result = func(*args)
elif args is not None:
    result = func(args)
else:
    result = func()
