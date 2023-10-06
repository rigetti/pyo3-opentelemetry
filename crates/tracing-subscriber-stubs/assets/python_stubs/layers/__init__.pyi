from typing import Union
from . import otlp, file, py_otlp


Config = Union[otlp.Config, file.Config, py_otlp.Config]

