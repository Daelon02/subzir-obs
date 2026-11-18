import unittest

from worker import CONTEXT_SECONDS, DialogueContext


class DialogueContextTests(unittest.TestCase):
    target = "I dunno, he's a funny guy."
    odd = "I dunno, he's a strange guy."

    def test_retry_keeps_context_but_next_distinct_line_replaces_it(self):
        context = DialogueContext()
        context.resolve("He gave us a fake name.", 0)
        self.assertEqual(context.resolve(self.target, 1), self.odd)
        self.assertEqual(context.resolve(self.target + "!", 2), self.odd + "!")
        context.resolve("Let's get dinner.", 3)
        self.assertEqual(context.resolve(self.target, 4), self.target)

    def test_timeout_and_new_session(self):
        context = DialogueContext()
        context.resolve("His story keeps changing.", 0)
        self.assertEqual(context.resolve(self.target, CONTEXT_SECONDS + 1), self.target)
        self.assertEqual(DialogueContext().resolve(self.target, 0), self.target)

    def test_current_humour_overrides_suspicion(self):
        context = DialogueContext()
        context.resolve("I don't trust him.", 0)
        text = "He's a funny guy. His jokes are great."
        self.assertEqual(context.resolve(text, 1), text)

    def test_negation_and_mixed_evidence(self):
        for previous in ("He isn't shady.", "He's shady but his jokes are great."):
            with self.subTest(previous=previous):
                context = DialogueContext()
                context.resolve(previous, 0)
                self.assertEqual(context.resolve(self.target, 1), self.target)
