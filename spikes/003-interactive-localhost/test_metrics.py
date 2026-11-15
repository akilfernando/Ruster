import os
import unittest
from unittest.mock import patch
import metrics


class MetricsTests(unittest.TestCase):
    def test_live_owned_cache_and_units(self):
        m = metrics.ProcessMetrics(os.getpid())
        a = m.sample()
        self.assertIs(m.sample(), a)
        for row in a['processes'].values():
            self.assertGreater(row['rss_bytes'], 1024)
            self.assertIsNone(row['cpu_percent_one_core'])

    def test_delta_one_core_and_reuse_denial(self):
        with patch.object(metrics, 'stat', return_value={'start': 1, 'ticks': 100, 'rss_bytes': 8192}), patch.object(metrics.time, 'monotonic', return_value=10):
            m = metrics.ProcessMetrics(os.getpid())
            m.sample()
        with patch.object(metrics, 'stat', return_value={'start': 1, 'ticks': 100 + os.sysconf('SC_CLK_TCK'), 'rss_bytes': 8192}), patch.object(metrics.time, 'monotonic', return_value=12):
            r = m.sample()['processes']['native']
            self.assertEqual(r['cpu_percent_one_core'], 50)
            self.assertEqual(r['interval_ms'], 2000)
            self.assertEqual(r['rss_bytes'], 8192)
        with patch.object(metrics, 'stat', return_value={'start': 2, 'ticks': 100, 'rss_bytes': 8192}), patch.object(metrics.time, 'monotonic', return_value=14):
            with self.assertRaises(ProcessLookupError):
                m.sample()


if __name__ == '__main__':
    unittest.main()
