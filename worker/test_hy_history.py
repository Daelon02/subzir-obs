import unittest

from hy_worker import echoes_history, retry_dialogue


class ContextEchoTests(unittest.TestCase):
    def test_retry_keeps_both_turns_and_allows_a_natural_split(self):
        def complete(messages):
            self.assertEqual(len(messages), 1)
            self.assertIn("I paid you back.\nCount it.", messages[0]["content"])
            self.assertTrue(messages[0]["content"].endswith("Are we square?"))
            return "Ну що? Ми квити?"
        self.assertEqual(retry_dialogue(complete, "Are we square?", "I paid you back.\nCount it.",
                                        "Ukrainian", {}), "Ну що? Ми квити?")

    def test_retry_rejects_repeated_prior_translation(self):
        self.assertIsNone(retry_dialogue(lambda _: "Перерахуй гроші. Ми квити?", "Are we square?",
            "Count it.", "Ukrainian", {"Count it.": "Перерахуй гроші."}))
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
