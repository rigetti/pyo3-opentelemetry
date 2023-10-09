from typing import Union
from .file import Config as FileConfig
{{#if layer_otel_file }}from .otel_otlp_file import Config as OtlpFileConfig{{/if}}
{{#if layer_otel_otlp}}from .otel_otlp import Config as OtlpConfig{{/if}}

Config = Union[
  FileConfig,
 {{#if layer_otel_file }}OtlpFileConfig,{{/if}} 
 {{#if layer_otel_otlp }}OtlpConfig,{{/if}}
]
