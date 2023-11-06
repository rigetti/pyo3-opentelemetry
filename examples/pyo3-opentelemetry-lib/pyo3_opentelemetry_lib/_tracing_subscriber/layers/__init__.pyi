from typing import Union
from .file import Config as FileConfig
from .otel_otlp_file import Config as OtlpFileConfig
from .otel_otlp import Config as OtlpConfig

Config = Union[
  FileConfig,
 OtlpFileConfig, 
 OtlpConfig,
]
"""
One of the supported layer configurations that may be set on the subscriber configuration.
"""
