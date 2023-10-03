from typing import List
from pyo3_opentelemetry_lib._tracing_subscriber import BatchConfig, GlobalTracingConfig, SimpleConfig, Tracing, CurrentThreadTracingConfig, TracingConfig, subscriber, layers
from pyo3_opentelemetry_lib._tracing_subscriber.layers import file, otlp, py_otlp
from pyo3_opentelemetry_lib._tracing_subscriber.layers.py_otlp import OtlpBytesExporter
import pytest


def _build_tracing_configs(layer: layers.Config) -> List[TracingConfig]:
    return [
        CurrentThreadTracingConfig(subscriber=subscriber.Config(layer=layer)),
        GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=layer))),
        GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=layer))),
    ]



@pytest.mark.parametrize("config", _build_tracing_configs(file.Config(file_path="./spans.txt"),))
async def test_file_export_tracing(config: TracingConfig):
    async with Tracing(config=config):
        pass


@pytest.mark.parametrize("config", _build_tracing_configs(otlp.Config(),))
async def test_otlp_export(config: TracingConfig):
    async with Tracing(config=config):
        pass


class PyOtlpExporter(OtlpBytesExporter):
    def export(self, serialized_resource_spans: List[bytes]) -> None:
        pass 



@pytest.mark.parametrize("config", _build_tracing_configs(py_otlp.Config(exporter=PyOtlpExporter()),))
async def test_py_otlp_export(config: TracingConfig):
    async with Tracing(config=config):
        pass
