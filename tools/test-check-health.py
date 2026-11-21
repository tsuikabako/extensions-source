#!/usr/bin/env python3
"""Tests of the pure rules of check-health.py. Run: python3 tools/test-check-health.py"""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("check_health", Path(__file__).with_name("check-health.py"))
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)


class Rules(unittest.TestCase):
    def test_an_answer_is_sorted_ok_challenge_or_fail(self):
        self.assertEqual(health.classify(200, {}, "<title>Weeb Central</title>", "weeb central"), "ok")
        self.assertEqual(health.classify(200, {}, "something else", "weeb central"), "fail", "the page lacks its text")
        self.assertEqual(health.classify(200, {}, "anything", ""), "ok", "no text expected")
        self.assertEqual(health.classify(403, {"cf-mitigated": "challenge"}, "Just a moment", "x"), "challenge")
        anubis = {"set-cookie": "__ddg1_=a; Path=/\ntecharo.lol-anubis-cookie-verification-347ddb4a=01a1; Secure"}
        self.assertEqual(health.classify(200, anubis, "Making sure you're not a bot!", "InkStory"), "challenge", "Anubis at 200")
        self.assertEqual(health.classify(200, {"set-cookie": "session=1"}, "<title>InkStory</title>", "InkStory"), "ok")
        self.assertEqual(health.classify(500, {}, "", "x"), "fail")
        self.assertEqual(health.classify(None, {}, "", "x"), "fail", "no answer at all")

    def test_one_failure_is_not_broken_but_two_in_a_row_are(self):
        state = health.next_state(("active", 0), "fail")
        self.assertEqual(state, ("active", 1))
        state = health.next_state(state, "fail")
        self.assertEqual(state, ("broken", 2))

    def test_an_ok_clears_it_and_a_challenge_changes_nothing(self):
        self.assertEqual(health.next_state(("broken", 3), "ok"), ("active", 0))
        self.assertEqual(health.next_state(("broken", 3), "challenge"), ("broken", 3))
        self.assertEqual(health.next_state(("active", 1), "challenge"), ("active", 1))

    def test_a_failure_names_the_server_the_cookies_and_the_page(self):
        headers = {"server": "ddos-guard", "set-cookie": "__ddg1_=a; Path=/\ntecharo.lol-anubis-cookie-verification-1=b; Secure"}
        body = "<html><head><title>Making sure\n you&#39;re not a bot!</title></head>"
        self.assertEqual(
            health.failure_detail(headers, body),
            "server='ddos-guard' cookies='__ddg1_,techaro.lol-anubis-cookie-verification-1' page='Making sure you&#39;re not a bot!'",
        )
        self.assertEqual(health.failure_detail({}, "Forbidden " * 40), f"server='' cookies='' page={('Forbidden ' * 20).strip()!r}")


if __name__ == "__main__":
    unittest.main()
