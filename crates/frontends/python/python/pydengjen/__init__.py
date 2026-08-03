# coding: utf-8

import os

os.putenv(
    "DENGJEN_ESPEAKNG_DATA_DIRECTORY",
    os.path.abspath(os.path.dirname(__file__))
)

from .pydengjen import *

__doc__ = pydengjen.__doc__
if hasattr(pydengjen, "__all__"):
    __all__ = pydengjen.__all__
    