from typing import Union
from . import file, otlp, py_otlp


LayerConfig = Union[file.Config, otlp.Config, py_otlp.Config]


class Config:
    def __init__(self, *, layer: LayerConfig):
        ... 
