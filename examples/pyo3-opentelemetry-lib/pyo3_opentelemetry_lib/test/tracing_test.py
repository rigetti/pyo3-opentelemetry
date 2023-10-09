from __future__ import annotations

import os
from base64 import urlsafe_b64encode
from collections import Counter
from time import time
from typing import TYPE_CHECKING, Any, Callable, Dict, List, MutableMapping, Optional

import pytest
from google.protobuf import json_format
from opentelemetry import propagate
from opentelemetry.context import attach, detach
from opentelemetry.proto.collector.trace.v1.trace_service_pb2 import ExportTraceServiceRequest
from opentelemetry.proto.trace.v1.trace_pb2 import ResourceSpans
from opentelemetry.trace import Tracer
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
from pyo3_opentelemetry_lib._tracing_subscriber.layers import otel_otlp as otlp
from pyo3_opentelemetry_lib._tracing_subscriber.layers import otel_otlp_file as file

if TYPE_CHECKING:
    from pyo3_opentelemetry_lib._tracing_subscriber import TracingConfig


_TEST_ARTIFACTS_DIR = os.path.join(os.path.dirname(__file__), "__artifacts__")


def require_force(param: Any):
    return pytest.param(
        param, marks=pytest.mark.skipif(not bool(os.getenv("PYTEST_FORCE", None)), reason="must force test")
    )


async def _test_file_export(config_builder: Callable[[str], TracingConfig], tracer: Tracer):
    filename = f"test_file_export-{time()}.txt"
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

    counter = Counter()
    for resource_span in resource_spans:
        for scoped_span in resource_span.scope_spans:
            for span in scoped_span.spans:
                span_trace_id = _16b_json_encoded_bytes_to_int(span.trace_id)
                assert span_trace_id is None or span_trace_id == trace_id, filename
                counter[span.name] += 1
    assert len(counter) == 1
    assert counter["example_function_impl"] == 1


@pytest.mark.parametrize(
    "config_builder",
    [
        lambda filename: CurrentThreadTracingConfig(
            export_process=SimpleConfig(
                subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename)))
            )
        ),
        lambda filename: CurrentThreadTracingConfig(
            export_process=BatchConfig(
                subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename)))
            )
        ),
        require_force(
            lambda filename: GlobalTracingConfig(
                export_process=SimpleConfig(
                    subscriber=subscriber.Config(
                        layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))
                    )
                )
            )
        ),
        require_force(
            lambda filename: GlobalTracingConfig(
                export_process=BatchConfig(
                    subscriber=subscriber.Config(
                        layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))
                    )
                )
            )
        ),
    ],
)
async def test_file_export(config_builder: Callable[[str], TracingConfig], tracer: Tracer, file_export_filter: None):
    await _test_file_export(config_builder, tracer)


@pytest.mark.parametrize(
    "config_builder",
    [
        lambda filename: CurrentThreadTracingConfig(
            export_process=SimpleConfig(
                subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename)))
            )
        ),
        lambda filename: CurrentThreadTracingConfig(
            export_process=BatchConfig(
                subscriber=subscriber.Config(layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename)))
            )
        ),
    ],
)
async def test_file_export_multi_threads(config_builder: Callable[[str], TracingConfig], tracer: Tracer, file_export_filter: None):
    for _ in range(3):
        await _test_file_export(config_builder, tracer)


@pytest.mark.parametrize(
    "config_builder",
    [
        require_force(
            lambda filename: GlobalTracingConfig(
                export_process=SimpleConfig(
                    subscriber=subscriber.Config(
                        layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))
                    )
                )
            )
        ),
        require_force(
            lambda filename: GlobalTracingConfig(
                export_process=BatchConfig(
                    subscriber=subscriber.Config(
                        layer=file.Config(file_path=os.path.join(_TEST_ARTIFACTS_DIR, filename))
                    )
                )
            )
        ),
    ],
)
async def test_file_export_async(config_builder: Callable[[str], TracingConfig], tracer: Tracer, file_export_filter: None):
    filename = f"test_file_export_async-{time()}.txt"
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
                span_trace_id = _16b_json_encoded_bytes_to_int(span.trace_id)
                assert span_trace_id is None or span_trace_id == trace_id, filename
                if span.name == "example_function_impl_async":
                    duration_ns = span.end_time_unix_nano - span.start_time_unix_nano
                    expected_duration_ms = 100
                    assert duration_ns > (expected_duration_ms * 10**6)
                    assert duration_ns < (1.5 * expected_duration_ms * 10**6)
    assert len(counter) == 2
    assert counter["example_function_impl"] == 1
    assert counter["example_function_impl_async"] == 1


def _16b_json_encoded_bytes_to_int(b: bytes) -> Optional[int]:
    decoded = urlsafe_b64encode(b).decode("utf-8")
    try:
        return int(decoded, 16)
    except ValueError:
        # 15769111199087022768103192234192075546.
        # https://github.com/open-telemetry/opentelemetry-rust/blob/6713143b59659dc509b7815404ebb57ad41cfe3a/opentelemetry-stdout/src/trace/transform.rs#L96
        # Rust: format(":x", 15769111199087022768103192234192075546) -> bdd05355c559cbb7c36ee676b58fb1a
        # Python: format(15769111199087022768103192234192075546, "032x") -> 0bdd05355c559cbb7c36ee676b58fb1a
        # Python will fail to base64 round trip the Rust encoded value because it is missing a leading 0.
        return None


@pytest.mark.parametrize(
    "config",
    [
        CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        require_force(
            GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config())))
        ),
        require_force(
            GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config())))
        ),
    ],
)
async def test_otlp_export(
    config: TracingConfig,
    tracer: Tracer,
    otlp_test_namespace: str,
    otlp_service_data: MutableMapping[str, List[ResourceSpans]],
):
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
    data = otlp_service_data.get(otlp_test_namespace, None)
    assert data is not None
    for resource_span in data:
        for scope_span in resource_span.scope_spans:
            for span in scope_span.spans:
                assert int.from_bytes(span.trace_id, "big") == trace_id, trace_id
                counter[span.name] += 1
    assert len(counter) == 1
    assert counter["example_function_impl"] == 1


@pytest.mark.parametrize(
    "config",
    [
        CurrentThreadTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
        CurrentThreadTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config()))),
    ],
)
async def test_otlp_export_multi_threads(
    config: TracingConfig,
    tracer: Tracer,
    otlp_test_namespace: str,
    otlp_service_data: MutableMapping[str, List[ResourceSpans]],
):
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

        data = otlp_service_data.get(otlp_test_namespace, None)
        assert data is not None
        counter = Counter()
        for resource_span in data:
            for scope_span in resource_span.scope_spans:
                for span in scope_span.spans:
                    if int.from_bytes(span.trace_id, "big") == trace_id:
                        counter[span.name] += 1
        assert len(counter) == 1
        assert counter["example_function_impl"] == 1


@pytest.mark.parametrize(
    "config",
    [
        require_force(
            GlobalTracingConfig(export_process=SimpleConfig(subscriber=subscriber.Config(layer=otlp.Config())))
        ),
        require_force(
            GlobalTracingConfig(export_process=BatchConfig(subscriber=subscriber.Config(layer=otlp.Config())))
        ),
    ],
)
async def test_otlp_export_async(
    config: TracingConfig,
    tracer: Tracer,
    otlp_test_namespace: str,
    otlp_service_data: MutableMapping[str, List[ResourceSpans]],
):
    async with Tracing(config=config):
        with tracer.start_as_current_span("test_file_export_tracing"):
            current_span = get_current_span()
            span_context = current_span.get_span_context()
            assert span_context.is_valid
            trace_id = span_context.trace_id
            assert trace_id != 0
            result = await pyo3_opentelemetry_lib.example_function_async()

    _assert_propagated_trace_id_eq(result, trace_id)

    data = otlp_service_data.get(otlp_test_namespace, None)
    assert data is not None
    counter = Counter()
    for resource_span in data:
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
