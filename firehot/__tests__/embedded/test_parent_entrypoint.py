import io
from unittest.mock import MagicMock

import pytest

from firehot.embedded.parent_entrypoint import MultiplexedStream


@pytest.fixture
def mock_stream():
    """Fixture providing a StringIO mock stream for capturing output."""
    return io.StringIO()


@pytest.fixture
def multiplexed_stream(mock_stream):
    """Fixture providing a MultiplexedStream with a fixed PID for testing."""
    stream = MultiplexedStream(mock_stream, "test_stream")
    # Set a fixed PID for testing
    stream.pid = 12345
    return stream


def test_write_single_line(mock_stream, multiplexed_stream):
    """Test writing a single line to the stream."""
    # Write a single line to the stream
    multiplexed_stream.write("Hello, world!")

    # Check the output has the correct prefix
    expected = "[PID:12345:test_stream]Hello, world!\n"
    assert mock_stream.getvalue() == expected


def test_write_multiple_lines(mock_stream, multiplexed_stream):
    """Test writing multiple lines to the stream."""
    # Write multiple lines to the stream
    multiplexed_stream.write("Line 1\nLine 2\nLine 3")

    # Check the output has the correct prefix for each line
    expected = (
        "[PID:12345:test_stream]Line 1\n"
        + "[PID:12345:test_stream]Line 2\n"
        + "[PID:12345:test_stream]Line 3\n"
    )
    assert mock_stream.getvalue() == expected


def test_write_empty_string(mock_stream, multiplexed_stream):
    """Test writing an empty string."""
    # Write an empty string
    multiplexed_stream.write("")

    # Should not write anything
    assert mock_stream.getvalue() == ""


def test_flush():
    """Test that flush is forwarded to the original stream."""
    # Create a mock object that tracks calls
    mock_stream = MagicMock()
    stream = MultiplexedStream(mock_stream, "test_stream")

    # Call flush
    stream.flush()

    # Verify flush was called on the original stream
    mock_stream.flush.assert_called_once()


def test_attribute_forwarding():
    """Test that other attributes are forwarded to the original stream."""
    # Create a mock object with a custom attribute
    mock_stream = MagicMock()
    mock_stream.custom_attr = "test_value"
    stream = MultiplexedStream(mock_stream, "test_stream")

    # Access the custom attribute
    value = stream.custom_attr

    # Verify we get the value from the original stream
    assert value == "test_value"
