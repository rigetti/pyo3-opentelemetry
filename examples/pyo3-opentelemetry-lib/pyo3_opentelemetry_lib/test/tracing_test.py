from __future__ import annotations

from base64 import b64encode
from collections import Counter
import os
from typing import TYPE_CHECKING, Callable, Dict, List, MutableSequence
from uuid import uuid4
import pytest
from google.protobuf import json_format
from opentelemetry import propagate
from opentelemetry.context import attach, detach
from opentelemetry.proto.collector.trace.v1.trace_service_pb2 import ExportTraceServiceRequest
from opentelemetry.proto.trace.v1.trace_pb2 import ResourceSpans
from opentelemetry.trace import Tracer, format_trace_id
from opentelemetry.trace.propagation import get_current_span

import pyo3_opentelemetry_lib
from pyo3_opentelemetry_lib._tracing_subscriber import (
    BatchConfig,
    CurrentThreadTracingConfig,
    GlobalTracingConfig,
    SimpleConfig,
    Tracing,
    subscriber,
)
from pyo3_opentelemetry_lib._tracing_subscriber.layers import otel_file as file, otel_otlp as otlp

if TYPE_CHECKING:
    from pyo3_opentelemetry_lib._tracing_subscriber import TracingConfig


_TEST_ARTIFACTS_DIR = os.path.join(os.path.dirname(__file__), "__artifacts__")


@pytest.mark.forked
@pytest.mark.parametrize(
    "config_builder",
    [
        lambda filename: CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
        lambda filename: CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
        lambda filename: GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
        lambda filename: GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
    ]
)
async def test_file_export(config_builder: Callable[[str], TracingConfig], tracer: Tracer):
    filename = f"{uuid4()}.txt"
    config = config_builder(filename)
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = pyo3_opentelemetry_lib.example_function()

    _assert_propagated_trace_id_eq(result, trace_id)

    file_path = os.path.join(_TEST_ARTIFACTS_DIR, filename)
    with open(file_path, "r") as f:
        resource_spans: List[ResourceSpans] = []
        for line in f.readlines():
            request = ExportTraceServiceRequest()
            json_format.Parse(line, request)
            resource_spans += request.resource_spans
    
    assert len(resource_spans) == 1

    counter = Counter()
    for resource_span in resource_spans:
        for scoped_span in resource_span.scope_spans:
            for span in scoped_span.spans:
                assert b64encode(span.trace_id).decode("utf-8") == format_trace_id(trace_id), trace_id
                counter[span.name] += 1
    assert len(counter) == 1
    assert counter["example_function_impl"] == 1


@pytest.mark.forked
@pytest.mark.parametrize(
    "config_builder",
    [
        lambda filename: CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
        lambda filename: CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
    ]
)
async def test_file_export_multi_threads(config_builder: Callable[[str], TracingConfig], tracer: Tracer):
    for _ in range(3):
        await test_file_export(config_builder, tracer)


@pytest.mark.forked
@pytest.mark.parametrize(
    "config_builder",
    [
        lambda filename: GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
        lambda filename: GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))))),
    ]
)
async def test_file_export_async(config_builder: Callable[[str], TracingConfig], tracer: Tracer):
    filename = f"{uuid4()}.txt"
    config = config_builder(filename)
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = await pyo3_opentelemetry_lib.example_function_async()

    _assert_propagated_trace_id_eq(result, trace_id)

    file_path = os.path.join(_TEST_ARTIFACTS_DIR, filename)
    with open(file_path, "r") as f:
        resource_spans: List[ResourceSpans] = []
        for line in f.readlines():
            request = ExportTraceServiceRequest()
            json_format.Parse(line, request)
            resource_spans += request.resource_spans
    
    counter = Counter()
    for resource_span in resource_spans:
        for scoped_span in resource_span.scope_spans:
            for span in scoped_span.spans:
                counter[span.name] += 1
                assert b64encode(span.trace_id).decode("utf-8") == format_trace_id(trace_id), trace_id
                if span.name == "example_function_impl_async":
                    duration_ns = span.end_time_unix_nano - span.start_time_unix_nano
                    expected_duration_ms = 100
                    assert duration_ns > (expected_duration_ms * 10**6)
                    assert duration_ns < (1.5 * expected_duration_ms * 10**6)
    assert len(counter) == 2
    assert counter["example_function_impl"] == 1 
    assert counter["example_function_impl_async"] == 1


@pytest.mark.forked
@pytest.mark.parametrize(
    "config",
    [
        CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
    ]
)
async def test_otlp_export(config: TracingConfig, tracer: Tracer, otlp_service: MutableSequence[ResourceSpans]):
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = pyo3_opentelemetry_lib.example_function()

    _assert_propagated_trace_id_eq(result, trace_id)
    
    counter = Counter()
    for resource_span in otlp_service:
        for scope_span in resource_span.scope_spans:
            for span in scope_span.spans:
                assert int.from_bytes(span.trace_id, "big") == trace_id, trace_id 
                counter[span.name] += 1
    assert len(counter) == 1
    assert counter["example_function_impl"] == 1


@pytest.mark.forked
@pytest.mark.parametrize(
    "config",
    [
        CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
    ]
)
async def test_otlp_export_multi_threads(config: TracingConfig, tracer: Tracer, otlp_service: MutableSequence[ResourceSpans]):
    for _ in range(3):
        async with Tracing(config=config):
            with tracer.start_as_current_span("test_file_export_tracing"):
                current_span = get_current_span()
                span_context = current_span.get_span_context()
                assert span_context.is_valid
                trace_id = span_context.trace_id
                assert trace_id != 0
                result = pyo3_opentelemetry_lib.example_function()

        _assert_propagated_trace_id_eq(result, trace_id)
        
        counter = Counter()
        for resource_span in otlp_service:
            for scope_span in resource_span.scope_spans:
                for span in scope_span.spans:
                    if int.from_bytes(span.trace_id, "big") == trace_id: 
                        counter[span.name] += 1
        assert len(counter) == 1
        assert counter["example_function_impl"] == 1


@pytest.mark.forked
@pytest.mark.parametrize(
    "config",
    [
        GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
    ]
)
async def test_otlp_export_async(config: TracingConfig, tracer: Tracer, otlp_service: MutableSequence[ResourceSpans]):
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = await pyo3_opentelemetry_lib.example_function_async()

    _assert_propagated_trace_id_eq(result, trace_id)
   
    counter = Counter()
    for resource_span in otlp_service:
        for scope_span in resource_span.scope_spans:
            for span in scope_span.spans:
                counter[span.name] += 1
                assert int.from_bytes(span.trace_id, "big") == trace_id, trace_id 
                if span.name == "example_function_impl_async":
                    duration_ns = span.end_time_unix_nano - span.start_time_unix_nano
                    expected_duration_ms = 100
                    assert duration_ns > (expected_duration_ms * 10**6)
                    assert duration_ns < (1.5 * expected_duration_ms * 10**6)
    assert len(counter) == 2
    assert counter["example_function_impl"] == 1 
    assert counter["example_function_impl_async"] == 1 


def _assert_propagated_trace_id_eq(carrier: Dict[str, str], trace_id: int):
    new_context = propagate.extract(carrier=carrier)
    token = attach(new_context)
    assert get_current_span().get_span_context().trace_id == trace_id
    detach(token)
