from dataclasses import dataclass
from typing import Any, Callable
from uuid import UUID

from firehot.firehot import (
    communicate_isolated as communicate_isolated_rs,
)
from firehot.firehot import (
    exec_isolated as exec_isolated_rs,
)
from firehot.firehot import (
    update_environment as update_environment_rs,
)
from firehot.naming import NAME_REGISTRY


@dataclass
class IsolatedProcess:
    process_uuid: UUID
    process_name: str


class ImportRunner:
    """
    A class that represents an isolated Python environment for executing code.
    """

    def __init__(self, runner_id: str):
        """
        Initialize the ImportRunner with a runner ID.

        Args:
            runner_id: The unique identifier for this runner
        """
        self.runner_id = runner_id

    def exec(self, func: Callable, *args: Any, name: str | None = None) -> IsolatedProcess:
        """
        Execute a function in the isolated environment.

        Args:
            func: The function to execute. A function should fully contain its content, including imports.
            *args: Arguments to pass to the function
            name: Optional name for the process

        Returns:
            An IsolatedProcess instance representing the execution
        """
        process_name = name or NAME_REGISTRY.reserve_random_name()
        exec_id = UUID(exec_isolated_rs(self.runner_id, process_name, func, args))
        return IsolatedProcess(process_uuid=exec_id, process_name=process_name)

    def communicate_isolated(self, isolate: IsolatedProcess | UUID) -> str:
        """
        Communicate with an isolated process to get its output

        Args:
            isolate: Either an IsolatedProcess instance or a UUID object

        Returns:
            The output from the isolated process
        """
        # Handle both IsolatedProcess objects and raw UUIDs
        process_uuid = isolate.process_uuid if isinstance(isolate, IsolatedProcess) else isolate
        return communicate_isolated_rs(self.runner_id, str(process_uuid))

    def update_environment(self):
        """
        Update the environment by checking for import changes and restarting if necessary
        """
        return update_environment_rs(self.runner_id)
