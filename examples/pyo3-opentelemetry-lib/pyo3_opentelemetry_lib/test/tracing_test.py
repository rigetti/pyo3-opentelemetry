from __future__ import annotations
import asyncio
from base64 import b64encode
from contextlib import asynccontextmanager
from multiprocessing.managers import ListProxy
from typing import Any, AsyncGenerator, Dict, List, TYPE_CHECKING

import multiprocessing as mp

from google.protobuf import json_format
from opentelemetry import propagate
from opentelemetry import trace
from opentelemetry.context import attach, detach
from opentelemetry.proto.trace.v1.trace_pb2 import ResourceSpans
from opentelemetry.proto.collector.trace.v1.trace_service_pb2 import ExportTraceServiceRequest
from opentelemetry.trace import Tracer, format_trace_id
from opentelemetry.trace.propagation import get_current_span
from pyo3_opentelemetry_lib._tracing_subscriber import BatchConfig, GlobalTracingConfig, SimpleConfig, Tracing, CurrentThreadTracingConfig, subscriber 
from pyo3_opentelemetry_lib._tracing_subscriber.layers import file, otlp, py_otlp
import pytest

import pyo3_opentelemetry_lib

from pyo3_opentelemetry_lib.test.conftest import TraceServiceServicer

if TYPE_CHECKING:
    from pyo3_opentelemetry_lib._tracing_subscriber import TracingConfig 
    from pyo3_opentelemetry_lib._tracing_subscriber.layers import Config as LayerConfig    


def _build_tracing_configs(layer: LayerConfig) -> List[TracingConfig]:
    return [
        # CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=layer))),
        # GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=layer))),
        GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=layer))),
    ]


FILE_EXPORT_PATH = "./spans.txt"


@pytest.mark.parametrize("config", _build_tracing_configs(file.Config(file_path=FILE_EXPORT_PATH),))
async def test_file_export_tracing(config: TracingConfig, tracer: Tracer):
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = pyo3_opentelemetry_lib.example_function()

    new_context = propagate.extract(carrier=result)
    token = attach(new_context)
    assert get_current_span().get_span_context().trace_id == trace_id
    detach(token)

    with open(FILE_EXPORT_PATH, "r") as f:
        resource_spans: List[ResourceSpans] = []
        for line in f.readlines():
            request = ExportTraceServiceRequest()
            json_format.Parse(line, request)
            resource_spans += request.resource_spans
    for resource_span in resource_spans:
        for scoped_span in resource_span.scope_spans:
            for span in scoped_span.spans:
                assert b64encode(span.trace_id).decode('utf-8') == format_trace_id(trace_id), trace_id


@asynccontextmanager
async def _start_as_current_span_async(tracer: Tracer, *args, **kwargs) -> AsyncGenerator[trace.Span, None]:
    """
    This function providers a decorator function for async functions that will start a span and set it as the current
    span.

    This is necessary because `tracer.Tracer.start_as_current_span` currently does not support asynchronous
    functions. See `opentelemetry-python#62 <https://github.com/open-telemetry/opentelemetry-python/issues/62>`_
    for more detail.

    :param tracer: The tracer to use to start the span.
    :param args: The arguments to pass to the tracer's `start_as_current_span` method.
    :param kwargs: The keyword arguments to pass to the tracer's `start_as_current_span` method.
    """
    with tracer.start_as_current_span(*args, **kwargs) as span:
        yield span


@pytest.mark.parametrize("config", _build_tracing_configs(otlp.Config(),))
async def test_otlp_export(config: TracingConfig, tracer: Tracer, otlp_service: ListProxy):
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = pyo3_opentelemetry_lib.example_function()
            await asyncio.sleep(5)

    new_context = propagate.extract(carrier=result)
    token = attach(new_context)
    assert get_current_span().get_span_context().trace_id == trace_id
    detach(token)

    assert len(otlp_service) == 99


class PyOtlpExporter:
    traces_data: List[ResourceSpans] = []

    def export(self, serialized_resource_spans: List[bytes]) -> None:
        for span_data in serialized_resource_spans: 
            resource_spans = ResourceSpans()
            resource_spans.ParseFromString(span_data)
            self.traces_data.append(resource_spans)


@pytest.mark.asyncio
async def test_py_otlp_export(tracer: Tracer):
    exporter = PyOtlpExporter()
    config = GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=py_otlp.Config(exporter=exporter))))

    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = pyo3_opentelemetry_lib.example_function()
            await asyncio.sleep(1)

    new_context = propagate.extract(carrier=result)
    token = attach(new_context)
    assert get_current_span().get_span_context().trace_id == trace_id
    detach(token)

    for resource_span in exporter.traces_data:
        for scoped_span in resource_span.scope_spans:
            for span in scoped_span.spans:
                assert int.from_bytes(span.trace_id, 'big') == trace_id

