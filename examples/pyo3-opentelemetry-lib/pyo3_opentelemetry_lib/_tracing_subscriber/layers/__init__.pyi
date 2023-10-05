from typing import Union

from .file import Config as FileConfig
from .otlp import Config as OtlpConfig

Config = Union[OtlpConfig, FileConfig]
