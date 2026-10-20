"""A heartbeat after a notification update must not replace that update."""
import unittest
from fixture_frames import latest_state_frame


class NotificationTransportFrames(unittest.TestCase):
    def test_redacted_update_followed_by_heartbeats(self):
        first = dict(instance='owner', revision=8, rows=[dict(id=1)])
        second = dict(instance='owner', revision=9, rows=[dict(id=1), dict(id=2)])
        heartbeat = dict(instance='owner', liveness=True, commandUntil=123)
        self.assertIs(latest_state_frame([first, second] + [heartbeat] * 40), second)
        self.assertEqual(latest_state_frame([heartbeat]), {})

    def test_later_failure_or_owner_state_does_not_reuse_old_rows(self):
        ready = dict(instance='old', rows=[dict(id=1), dict(id=2)])
        for state in ({'available': False, 'error': 'owner lost'},
                      {'instance': 'new', 'rows': []}, {'liveness': False}):
            with self.subTest(state=state):
                self.assertIs(latest_state_frame([ready, state, {'liveness': True}]), state)

    def test_malformed_frame_is_not_silently_skipped(self):
        with self.assertRaises(ValueError):
            latest_state_frame([{'rows': [1, 2]}, []])


if __name__ == '__main__':
    unittest.main()
