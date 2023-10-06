from typing import Union
from .otel_file import Config as FileConfig
from .otel_otlp import Config as OtlpConfig


Config = Union[FileConfig, OtlpConfig]

