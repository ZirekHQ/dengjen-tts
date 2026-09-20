import inspect
import os
import subprocess
import sys
import unittest
from pathlib import Path

import pydengjen

DATA_ENV = "DENGJEN_ESPEAKNG_DATA_DIRECTORY"
PACKAGE_DIR = Path(pydengjen.__file__).resolve().parent
PROBE = (
    "import ctypes, pydengjen\n"
    "libc = ctypes.CDLL(None)\n"
    "libc.getenv.restype = ctypes.c_char_p\n"
    f"print((libc.getenv({DATA_ENV.encode()!r}) or b'').decode())\n"
)


def defaults(callable_):
    parameters = inspect.signature(callable_).parameters.values()
    return {p.name: p.default for p in parameters if p.default is not inspect.Parameter.empty}


def data_directory_seen_by_a_fresh_interpreter(value):
    env = {k: v for k, v in os.environ.items() if k != DATA_ENV}
    if value is not None:
        env[DATA_ENV] = value
    result = subprocess.run(
        [sys.executable, "-c", PROBE], env=env, capture_output=True, text=True, check=True
    )
    return result.stdout.strip()


class OptionalArgumentTests(unittest.TestCase):
    def test_audio_output_config_needs_no_arguments(self):
        pydengjen.AudioOutputConfig()
        pydengjen.AudioOutputConfig(rate=60)

    def test_synthesis_methods_default_their_optional_arguments_to_none(self):
        expected = {
            "synthesize": ["audio_output_config"],
            "synthesize_lazy": ["audio_output_config"],
            "synthesize_parallel": ["audio_output_config"],
            "synthesize_batched": ["audio_output_config", "batch_size"],
            "synthesize_streamed": ["audio_output_config", "chunk_size", "chunk_padding"],
            "synthesize_to_file": ["audio_output_config"],
        }
        for name, optional in expected.items():
            with self.subTest(method=name):
                found = defaults(getattr(pydengjen.Dengjen, name))
                self.assertEqual(found, {key: None for key in optional})

    def test_phonemize_text_defaults_everything_after_the_language_to_none(self):
        names = ["phoneme_separator", "remove_lang_switch_flags", "remove_stress", "use_tashkeel"]
        self.assertEqual(defaults(pydengjen.phonemize_text), {name: None for name in names})


class PiperScalesTests(unittest.TestCase):
    def test_values_are_readable(self):
        scales = pydengjen.PiperScales(1.0, 0.5, 0.25)
        self.assertEqual((scales.length_scale, scales.noise_scale, scales.noise_w), (1.0, 0.5, 0.25))


@unittest.skipIf(sys.platform == "win32", "the probe reads the C environment through libc")
class EspeakDataTests(unittest.TestCase):
    def test_the_package_ships_the_data(self):
        self.assertTrue((PACKAGE_DIR / "espeak-ng-data" / "phontab").is_file())

    def test_directory_defaults_to_the_package(self):
        self.assertEqual(data_directory_seen_by_a_fresh_interpreter(None), str(PACKAGE_DIR))

    def test_a_directory_the_user_set_is_kept(self):
        self.assertEqual(data_directory_seen_by_a_fresh_interpreter("/somewhere/else"), "/somewhere/else")


if __name__ == "__main__":
    unittest.main()
