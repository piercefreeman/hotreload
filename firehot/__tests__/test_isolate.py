import time
from uuid import UUID

import pytest

from firehot.environment import ImportRunner, isolate_imports


@pytest.fixture
def import_runner(sample_package):
    with isolate_imports(sample_package) as runner:
        yield runner


def function_with_exception():
    raise ValueError("This is a deliberate test exception")


def function_with_success(name):
    return f"Hello, {name}!"


def test_successful_execution(import_runner: ImportRunner):
    """Test that we can successfully execute a function in isolation."""

    # Execute the function in isolation
    process_uuid = import_runner.exec(function_with_success, "World")

    # Ensure we got a valid UUID
    assert isinstance(process_uuid, UUID)

    # Give it a moment to complete
    time.sleep(0.1)

    # Get the result
    result = import_runner.communicate_isolated(process_uuid)

    # Verify the result
    assert result == "Hello, World!"


def test_exception_in_child_process(import_runner: ImportRunner):
    """Test that exceptions in child processes are properly handled."""

    # Execute the function in isolation
    process_uuid = import_runner.exec(function_with_exception)

    # Ensure we got a valid UUID
    assert isinstance(process_uuid, UUID)

    # Give it a moment to fail
    time.sleep(0.1)

    # Try to get the result, which should raise an exception
    with pytest.raises(Exception) as excinfo:
        import_runner.communicate_isolated(process_uuid)

    # Verify the exception contains our error message
    assert "This is a deliberate test exception" in str(excinfo.value)
