import unittest

from hy_worker import echoes_history


class ContextEchoTests(unittest.TestCase):
    def test_natural_sentence_split(self):
        self.assertFalse(echoes_history(
            "Та годі. Ходімо!", "Come on, let's go.", "Wait here.", {"Wait here.": "Почекай тут."}
        ))

    def test_previous_subtitle_leaked_with_new_dialogue(self):
        self.assertTrue(echoes_history(
            "Ти йому віриш? Він прикидається.", "He's fronting.", "Do you believe him?",
            {"Do you believe him?": "Ти йому віриш?"}
        ))

    def test_intentional_repetition_in_source(self):
        self.assertFalse(echoes_history(
            "Почекай тут. Я повернуся.", "Wait here. I'll be back.", "Wait here.",
            {"Wait here.": "Почекай тут."}
        ))


if __name__ == "__main__":
    unittest.main()
