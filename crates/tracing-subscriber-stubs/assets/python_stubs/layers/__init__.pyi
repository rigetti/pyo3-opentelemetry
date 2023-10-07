{{#if any_layer}}from typing import Union{{/if}}
{{#if layer_otel_file }}from .otel_file import Config as FileConfig{{/if}}
{{#if layer_otel_otlp}}from .otel_otlp import Config as OtlpConfig{{/if}}

 {{#if any_layer }}
Config = Union[
 {{#if layer_otel_file }}FileConfig,{{/if}} 
 {{#if layer_otel_otlp }}OtlpConfig,{{/if}}
]
{{else}}
Config = None
{{/if}}
