import random
from random import seed

import pytest

from firehot.naming import NameManager


@pytest.fixture
def name_manager():
    """Fixture providing a basic NameManager instance."""
    return NameManager()


@pytest.fixture
def small_name_manager():
    """Fixture providing a NameManager with a small set of test words."""
    return NameManager(phrases=(["red", "blue", "green"], ["apple", "banana"]))


@pytest.fixture
def very_small_name_manager():
    """Fixture providing a NameManager with a very small set of test words."""
    return NameManager(phrases=(["red", "blue"], ["apple", "banana"]))


def test_custom_separator():
    """Test that custom separators are correctly initialized and used."""
    nm = NameManager(separator="_")
    assert nm.separator == "_"

    # Check name format with custom separator
    name = nm.reserve_random_name()
    assert "_" in name
    assert "-" not in name

    # Force loop increment
    nm.loop_count = 1  # Set to 1 first
    nm.current_pointers[0] = len(nm.word_lists[0])  # Force a loop initialization on next reserve
    name = nm.reserve_random_name()

    # Check loop count is appended with correct separator (after loop_count becomes 2)
    assert name.endswith("_2")


def test_reserve_random_name_format(name_manager: NameManager):
    """Test that reserved names have the correct format."""
    # First loop names should be word1-word2
    name = name_manager.reserve_random_name()
    parts = name.split(name_manager.separator)
    assert len(parts) == 2
    assert parts[0] in name_manager.word_lists[0]
    assert parts[1] in name_manager.word_lists[1]


def test_reserve_random_name_with_loop_count(name_manager: NameManager):
    """Test that names include loop counter when loop_count > 1."""
    # Force loop count to 2
    name_manager.loop_count = 2
    name = name_manager.reserve_random_name()
    parts = name.split(name_manager.separator)
    assert len(parts) == 3
    assert parts[0] in name_manager.word_lists[0]
    assert parts[1] in name_manager.word_lists[1]
    assert parts[2] == "2"


def test_initialize_loop(name_manager: NameManager):
    """Test that initialize_loop correctly resets and updates state."""
    # Set some non-default values
    name_manager.current_pointers = [5, 10]
    old_word_lists = [list.copy() for list in name_manager.word_lists]

    # Call initialize_loop
    seed(20)
    name_manager.initialize_loop()

    # Check that pointers were reset
    assert name_manager.current_pointers == [0, 0]

    # Check that loop count was incremented
    assert name_manager.loop_count == 1

    # Check that lists were reshuffled
    is_different = any(
        old_list != new_list
        for old_list, new_list in zip(old_word_lists, name_manager.word_lists, strict=False)
    )
    assert is_different, "Lists should be reshuffled"


def test_all_combinations_before_loop(small_name_manager: NameManager, monkeypatch):
    """Test that all combinations are used before initializing a new loop."""
    # Mock the random.shuffle to do nothing (maintain predictable order)
    monkeypatch.setattr(random, "shuffle", lambda x: None)

    # Get all combinations
    all_names = []
    for _ in range(small_name_manager.total_options):
        all_names.append(small_name_manager.reserve_random_name())

    # Check that we got all possible combinations
    expected_combinations = [
        f"{word1}{small_name_manager.separator}{word2}"
        for word1 in small_name_manager.word_lists[0]
        for word2 in small_name_manager.word_lists[1]
    ]

    assert sorted(all_names) == sorted(expected_combinations)

    # Check that the next name triggers a new loop
    # The loop_count will be 1 after first initialization, so we need to force it to be 1
    # so the next initialization makes it 2
    small_name_manager.loop_count = 1
    next_name = small_name_manager.reserve_random_name()
    assert small_name_manager.loop_count == 2
    assert next_name.endswith(f"{small_name_manager.separator}2")


def test_loop_increment(name_manager: NameManager):
    """Test that loop_count properly increments and affects names."""
    # Force first pointer to trigger loop initialization
    name_manager.current_pointers[0] = len(name_manager.word_lists[0])

    # Get a name - should trigger initialize_loop
    name = name_manager.reserve_random_name()

    # Check that loop_count was incremented
    assert name_manager.loop_count == 1

    # Force another loop initialization
    name_manager.current_pointers[0] = len(name_manager.word_lists[0])
    name = name_manager.reserve_random_name()

    # Check incrementing again
    assert name_manager.loop_count == 2
    assert name.endswith(f"{name_manager.separator}2")


def test_no_duplicates_in_single_loop():
    """Test that no duplicate names are generated within a single loop."""
    # Use small test set for faster testing
    nm = NameManager(phrases=(["red", "blue", "green", "yellow"], ["apple", "banana", "cherry"]))

    # Generate all names in one loop
    names = set()
    for _ in range(nm.total_options):
        name = nm.reserve_random_name()
        assert name not in names, f"Duplicate name generated: {name}"
        names.add(name)

    # Check we got the expected number of unique names
    assert len(names) == nm.total_options


def test_exhaustion_recovery(very_small_name_manager: NameManager):
    """Test that we can generate names beyond the total number of combinations."""
    nm = very_small_name_manager

    # Set loop count to 1 to make sure we get a suffix on the next loop
    nm.loop_count = 1

    # Generate more names than possible combinations
    total_to_generate = nm.total_options * 2 + 1
    names = [nm.reserve_random_name() for _ in range(total_to_generate)]

    # We should have unique names (within their respective loops)
    first_loop_names = names[: nm.total_options]
    second_loop_names = [
        name.removesuffix(f"{nm.separator}2")
        for name in names[nm.total_options : nm.total_options * 2]
    ]

    # Check all names in first loop are unique
    assert len(set(first_loop_names)) == len(first_loop_names)

    # Check all names in second loop are unique
    assert len(set(second_loop_names)) == len(second_loop_names)

    # Check that we got at least one name from the third loop
    assert any(name.endswith(f"{nm.separator}3") for name in names)


def test_index_updates_correctly(small_name_manager: NameManager):
    """Test that indices update correctly after each name generation."""
    nm = small_name_manager

    # Initial state
    assert nm.current_pointers[0] == 0
    assert nm.current_pointers[1] == 0

    # Get first name
    nm.reserve_random_name()
    assert nm.current_pointers[0] == 0
    assert nm.current_pointers[1] == 1

    # Get second name
    nm.reserve_random_name()
    assert nm.current_pointers[0] == 0

    # After the second reservation, the second pointer becomes 2
    # which might be out of bounds, but that's handled in the next call
    assert nm.current_pointers[1] == 2

    # Get third name - this will handle the overflow in the second pointer
    nm.reserve_random_name()

    # After third name, first pointer should increment and second pointer gets reset or wrapped
    assert nm.current_pointers[0] == 1
    assert nm.current_pointers[1] == 1
