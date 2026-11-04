import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

path = Path(__file__).resolve().parents[1] / "src/native/ai/worker.py"
spec = importlib.util.spec_from_file_location("worker", path)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class PrewarmedSynthesis(unittest.TestCase):
    def test_live_formant_shape_uses_eager_and_can_return_to_the_prepared_graph(self):
        graphs = []
        def run_graph(model, shape, forward, value):
            graphs.append(shape)
            return forward(value)

        sizes = []
        model = SimpleNamespace(_mic_synthesis_window=(150, 5),
                                infer=lambda value, skip, length, size: sizes.append(size))
        shapes = ["original", "edited", "original"]
        audio = [worker.prewarmed_synthesis(run_graph, model, shape, abs, -n)
                 for n, shape in enumerate(shapes, 1)]
        self.assertEqual((graphs, audio), (["original", "original"], [1, 2, 3]))
        self.assertEqual(sizes, [3, 4, 6, 7, 8, 9, 10])


if __name__ == "__main__":
    unittest.main()
