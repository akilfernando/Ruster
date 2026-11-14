import importlib.util
import pathlib
import unittest

class Boundary(unittest.TestCase):
    def test_exact_authority_and_secret(self):
        spec = importlib.util.spec_from_file_location('dashboard', pathlib.Path(__file__).with_name('server.py'))
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        good = {'Host':'127.0.0.1:54321', 'Origin':'http://127.0.0.1:54321', 'X-Spike-Capability':'secret'}
        self.assertTrue(module.authorized(good, '127.0.0.1:54321', 'secret'))
        for key,value in [('Host','evil.test'),('Origin','http://evil.test'),('X-Spike-Capability','wrong')]:
            self.assertFalse(module.authorized({**good,key:value}, '127.0.0.1:54321', 'secret'))
        for key in good:
            self.assertFalse(module.authorized({k:v for k,v in good.items() if k != key}, '127.0.0.1:54321', 'secret'))

if __name__ == '__main__': unittest.main()
