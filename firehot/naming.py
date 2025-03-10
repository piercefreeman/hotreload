import random

ADJECTIVES: list[str] = [
    "bright",
    "colorful",
    "ancient",
    "gentle",
    "brave",
    "clever",
    "dazzling",
    "eager",
    "fierce",
    "graceful",
    "happy",
    "infinite",
    "jolly",
    "kind",
    "luminous",
    "mystical",
    "noble",
    "peaceful",
    "quick",
    "radiant",
    "silent",
    "thoughtful",
    "unique",
    "vibrant",
    "wise",
    "zesty",
    "calm",
    "elegant",
    "golden",
    "humble",
]

NOUNS: list[str] = [
    "windmill",
    "flower",
    "mountain",
    "river",
    "forest",
    "ocean",
    "castle",
    "village",
    "garden",
    "horizon",
    "island",
    "journey",
    "kingdom",
    "lake",
    "meadow",
    "notebook",
    "orchard",
    "pathway",
    "quilt",
    "rainbow",
    "sunset",
    "treasure",
    "valley",
    "waterfall",
    "cloud",
    "bridge",
    "desert",
    "harbor",
    "lighthouse",
    "canyon",
]


class NameManager:
    """
    Memory efficient, non-duplicating name manager.

    """

    word_lists: list[list[str]]
    """
    Shuffled copies of the word lists, can create a random name by getting the next
    unused permutation of the lists.
    """

    current_pointers: list[int]
    """
    Current pointer to each word in the list.
    """

    def __init__(self, phrases: tuple[list[str], ...] | None = None, separator: str = "-"):
        self.separator = separator

        if phrases is None:
            phrases = (ADJECTIVES, NOUNS)

        self.word_lists = [list.copy() for list in phrases]

        # Maximum possible combinations. We'll reset the loop when we reach this number
        # of combinations.
        self.total_options = 1
        for word_list in self.word_lists:
            self.total_options *= len(word_list)

        # [1, ...]
        self.loop_count = -1
        self.initialize_loop()

    def initialize_loop(self) -> None:
        """
        Initialize a new loop through the combinations:
        - Reset indices to the beginning
        - Reshuffle the adjectives and nouns lists
        - Increment the loop counter for suffix addition

        """
        # Reset indices to beginning
        self.current_pointers = [0] * len(self.word_lists)

        # Reshuffle the lists for new random combinations
        for word_list in self.word_lists:
            random.shuffle(word_list)

        # Increment loop count for name suffixes
        self.loop_count += 1

    def reserve_random_name(self) -> str:
        """
        Reserve a random name using the counter-based approach.
        Ensures uniqueness across multiple exec runs.
        """
        # We need to check each word list in inverse order. Once we hit a max, we should increase
        # the previous index
        for i in range(len(self.word_lists) - 1, 0, -1):
            if self.current_pointers[i] >= len(self.word_lists[i]):
                self.current_pointers[i] = 0
                self.current_pointers[i - 1] += 1

        # Check if we've exhausted all combinations in this loop
        if self.current_pointers[0] >= len(self.word_lists[0]):
            self.initialize_loop()

        # Generate name from current indices
        current_words = [
            self.word_lists[i][self.current_pointers[i]] for i in range(len(self.word_lists))
        ]

        # Add loop count suffix if we've gone beyond the first loop
        if self.loop_count > 1:
            base_name = f"{self.separator.join(current_words)}"
            name = f"{base_name}{self.separator}{self.loop_count}"
        else:
            name = f"{self.separator.join(current_words)}"

        # Update indices for next call. We'll perform the full validation
        # at the start of the next loop.
        self.current_pointers[-1] += 1
        return name


# Singleton instance
NAME_REGISTRY = NameManager()
