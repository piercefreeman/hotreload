"""
Child entrypoint for hotreload.

Intended for embeddable usage in Rust, can only import stdlib modules.

"""

import base64
import importlib
import importlib.util
import logging
import pickle
import sys
from os import getenv
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from firehot.embedded.types import SerializedCall

    module_path = "path"
    pickled_str = "pickled_str"

# These will imported dynamically by rust
module_path: str
pickled_str: str

known_log_levels = {
    "TRACE": logging.DEBUG,
    "DEBUG": logging.DEBUG,
    "INFO": logging.INFO,
    "WARNING": logging.WARNING,
    "ERROR": logging.ERROR,
}
dynamic_imports = sys.argv[1] if len(sys.argv) > 1 else ""
log_level = known_log_levels.get(getenv("FIREHOT_LOG_LEVEL", "WARNING"), logging.WARNING)
logging.basicConfig(level=log_level)

# Decode base64 and unpickle
pickled_bytes = base64.b64decode(pickled_str)
data: "SerializedCall" = pickle.loads(pickled_bytes)

# If we have a module path, import it first to ensure the function is available
# Technically we could just unpickle the data and pickle will automatically try to resolve the module, but
# this lets us more explicitly handle errors and issue debugging logs.
module_path = data["func_module_path"]
if module_path:
    logging.info(f"Importing module: {module_path}")
    sys.stdout.flush()
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
