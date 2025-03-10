"""
This is the main entrypoint for our continuously running parent process. It will receive commands
from Rust and execute them.

Intended for embeddable usage in Rust, can only import stdlib modules.

"""

import logging
import os
import sys
from dataclasses import asdict, dataclass
from enum import StrEnum
from json import dumps as json_dumps
from json import loads as json_loads
from json.decoder import JSONDecodeError
from os import getenv
from time import sleep
from traceback import format_exc

#
# Messages
#


class MessageType(StrEnum):
    FORK_REQUEST = "FORK_REQUEST"
    FORK_RESPONSE = "FORK_RESPONSE"
    CHILD_COMPLETE = "CHILD_COMPLETE"
    CHILD_ERROR = "CHILD_ERROR"
    UNKNOWN_COMMAND = "UNKNOWN_COMMAND"
    UNKNOWN_ERROR = "UNKNOWN_ERROR"
    IMPORT_ERROR = "IMPORT_ERROR"
    IMPORT_COMPLETE = "IMPORT_COMPLETE"
    EXIT_REQUEST = "EXIT_REQUEST"


class MessageBase:
    name: MessageType


# Requests


@dataclass
class ForkRequest(MessageBase):
    request_id: str
    code: str
    request_name: str
    name: MessageType = MessageType.FORK_REQUEST


@dataclass
class ExitRequest(MessageBase):
    name: MessageType = MessageType.EXIT_REQUEST


# Responses


@dataclass
class ForkResponse(MessageBase):
    request_id: str
    request_name: str

    child_pid: int

    name: MessageType = MessageType.FORK_RESPONSE


@dataclass
class ChildComplete(MessageBase):
    result: str | None

    name: MessageType = MessageType.CHILD_COMPLETE


@dataclass
class ChildError(MessageBase):
    error: str
    traceback: str | None

    name: MessageType = MessageType.CHILD_ERROR


@dataclass
class UnknownCommandError(MessageBase):
    command: str

    name: MessageType = MessageType.UNKNOWN_COMMAND


@dataclass
class UnknownError(MessageBase):
    error: str
    traceback: str | None

    name: MessageType = MessageType.UNKNOWN_ERROR


@dataclass
class ImportError(MessageBase):
    error: str
    traceback: str | None

    name: MessageType = MessageType.IMPORT_ERROR


@dataclass
class ImportComplete(MessageBase):
    name: MessageType = MessageType.IMPORT_COMPLETE


MESSAGES = {
    MessageType.FORK_REQUEST: ForkRequest,
    MessageType.FORK_RESPONSE: ForkResponse,
    MessageType.CHILD_COMPLETE: ChildComplete,
    MessageType.CHILD_ERROR: ChildError,
    MessageType.UNKNOWN_COMMAND: UnknownCommandError,
    MessageType.UNKNOWN_ERROR: UnknownError,
    MessageType.IMPORT_ERROR: ImportError,
    MessageType.IMPORT_COMPLETE: ImportComplete,
    MessageType.EXIT_REQUEST: ExitRequest,
}


def write_message(message: MessageBase):
    sys.stdout.write(f"{json_dumps(asdict(message))}\n")
    sys.stdout.flush()


def read_message() -> MessageBase | None:
    line = sys.stdin.readline().strip()
    if not line:
        return None

    # Try to parse it as a JSON object
    try:
        payload = json_loads(line)
    except JSONDecodeError:
        return None

    # Get the name from the payload
    name = payload.get("name")
    if not name:
        return None

    try:
        message_type = MessageType(name)
    except ValueError:
        return None

    return MESSAGES[message_type](**payload)


#
# Logging
#


class MultiplexedStream:
    def __init__(self, original_stream, stream_name: str):
        self.original_stream = original_stream
        self.stream_name = stream_name
        self.pid = os.getpid()

    def write(self, text: str) -> int:
        # Add PID prefix to each line (newlines and lines with values)
        prefix = f"[PID:{self.pid}:{self.stream_name}]"

        text = text.strip()
        if not text:
            return

        prefixed_text = ""
        lines = text.split("\n")
        for line in lines:
            prefixed_text += f"{prefix}{line}\n"
        return self.original_stream.write(prefixed_text)

    def flush(self) -> None:
        return self.original_stream.flush()

    # Forward all other attributes to the original stream
    def __getattr__(self, attr):
        return getattr(self.original_stream, attr)


#
# Main Logic
#


def main():
    # This will be populated with dynamic import statements from Rust
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

    # Execute the dynamic imports
    try:
        if dynamic_imports:
            exec(dynamic_imports)
    except Exception as e:
        write_message(ImportError(error=str(e), traceback=format_exc()))
        sys.exit(1)

    # Signal that imports are complete
    write_message(ImportComplete())

    # Function to handle forking and executing code
    def handle_fork_request(code_to_execute):
        pid = os.fork()
        if pid == 0:
            # Child process
            try:
                # Set up globals and locals for execution
                exec_globals = globals().copy()
                exec_locals = {}

                # Immediately route stdout and stderr to a custom convention that specifies the
                # current pid, so our rust watcher can separate them from the single stdout stream
                # that's inherited from the parent environment

                # Save original stdout and stderr
                original_stdout = sys.stdout
                original_stderr = sys.stderr

                # Replace with PID-prefixed versions
                sys.stdout = MultiplexedStream(original_stdout, "stdout")
                sys.stderr = MultiplexedStream(original_stderr, "stderr")

                logging.info("Will execute code in forked process...")
                sys.stdout.flush()

                # Execute the code
                exec(code_to_execute, exec_globals, exec_locals)

                logging.info("Executed code in forked process")
                sys.stdout.flush()

                # By convention, the result is stored in the 'result' variable
                if "result" in exec_locals:
                    write_message(ChildComplete(result=str(exec_locals["result"])))
                else:
                    write_message(ChildComplete(result=None))
            except Exception as e:
                write_message(ChildError(error=str(e), traceback=format_exc()))
            finally:
                # Exit the child process
                sys.exit(0)
        else:
            # Parent process. The PID will represent the child process.
            return pid

    # Main loop - wait for commands on stdin
    while True:
        try:
            command = read_message()
            if not command:
                sleep(0.1)
                continue

            if isinstance(command, ForkRequest):
                fork_pid = handle_fork_request(command.code)
                write_message(
                    ForkResponse(
                        request_id=command.request_id,
                        request_name=command.request_name,
                        child_pid=fork_pid,
                    )
                )
            elif isinstance(command, ExitRequest):
                logging.info("Exiting loader process")
                sys.stdout.flush()
                break
            else:
                write_message(UnknownCommandError(command=str(command)))
        except Exception as e:
            write_message(UnknownError(error=str(e), traceback=format_exc()))


if __name__ == "__main__":
    main()
