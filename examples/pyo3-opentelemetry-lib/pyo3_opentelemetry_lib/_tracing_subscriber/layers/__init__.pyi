from typing import Union
from .file import Config as FileConfig

from .otel_otlp import Config as OtlpConfig

Config = Union[
  FileConfig,
  
 OtlpConfig,
]
