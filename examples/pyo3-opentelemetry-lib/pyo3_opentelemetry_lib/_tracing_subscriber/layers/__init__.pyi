from typing import Union

from .file import Config as FileConfig
from .otel_otlp import Config as OtlpConfig
from .otel_otlp_file import Config as OtlpFileConfig

Config = Union[
    FileConfig,
    OtlpFileConfig,
    OtlpConfig,
]
