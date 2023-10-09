##############################################################################
# Copyright 2023 Rigetti Computing
#
#    Licensed under the Apache License, Version 2.0 (the "License");
#    you may not use this file except in compliance with the License.
#    You may obtain a copy of the License at
#
#        http://www.apache.org/licenses/LICENSE-2.0
#
#    Unless required by applicable law or agreed to in writing, software
#    distributed under the License is distributed on an "AS IS" BASIS,
#    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#    See the License for the specific language governing permissions and
#    limitations under the License.
##############################################################################
import asyncio
import multiprocessing as mp
import os
import socket
from concurrent import futures
from typing import AsyncGenerator, Generator, List, MutableMapping
from unittest import mock
from uuid import uuid4

import grpc
import pytest
from grpc.aio import Metadata, ServicerContext, insecure_channel
from grpc.aio import server as create_grpc_server
from opentelemetry import trace
from opentelemetry.proto.collector.trace.v1 import trace_service_pb2, trace_service_pb2_grpc
from opentelemetry.proto.trace.v1.trace_pb2 import ResourceSpans
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import (
    BatchSpanProcessor,
    ConsoleSpanExporter,
)

mp.set_start_method("fork")


@pytest.fixture(scope="session")
def tracer() -> trace.Tracer:
    provider = TracerProvider()
    processor = BatchSpanProcessor(ConsoleSpanExporter())
    provider.add_span_processor(processor)
    return provider.get_tracer("integration-test")


class TraceServiceServicer(trace_service_pb2_grpc.TraceServiceServicer):
    def __init__(self, data: MutableMapping[str, List[ResourceSpans]]):
        self.lock = asyncio.Lock()
        self.resource_spans = data

    def _are_headers_set(self, metadata: Metadata) -> bool:
        for k, v in _SERVICE_TEST_HEADERS.items():
            value = next((value for key, value in metadata if key == k), None)
            if value != v:
                return False
        return True

    async def Export(
        self, request: trace_service_pb2.ExportTraceServiceRequest, context: ServicerContext
    ) -> trace_service_pb2.ExportTraceServiceResponse:
        metadata = context.invocation_metadata()
        if metadata is None or not self._are_headers_set(metadata):
            context.set_code(grpc.StatusCode.PERMISSION_DENIED)
            return trace_service_pb2.ExportTraceServiceResponse()

        namespace = next((value for key, value in metadata if key == "x-test-namespace"), None)
        if namespace is None:
            context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            return trace_service_pb2.ExportTraceServiceResponse()
        namespace = namespace.decode("utf-8") if isinstance(namespace, bytes) else str(namespace)
        async with self.lock:
            if namespace not in self.resource_spans:
                self.resource_spans[namespace] = []
            self.resource_spans[namespace] += list(request.resource_spans)
        context.set_code(grpc.StatusCode.OK)
        return trace_service_pb2.ExportTraceServiceResponse(
            partial_success=trace_service_pb2.ExportTracePartialSuccess()
        )


_SERVICE_TEST_HEADERS = {
    "header1": "one",
    "header2": "two",
}


async def _start_otlp_service_async(data, port):
    server = create_grpc_server(
        futures.ThreadPoolExecutor(max_workers=1),
    )
    servicer = TraceServiceServicer(data)
    trace_service_pb2_grpc.add_TraceServiceServicer_to_server(servicer, server)

    server.add_insecure_port(f"[::]:{port}")
    try:
        await server.start()
        await server.wait_for_termination()
    except Exception as e:
        print(e)


def _start_otlp_service(data, port):
    asyncio.run(_start_otlp_service_async(data, port))


@pytest.fixture(scope="session")
def event_loop():
    loop = asyncio.get_event_loop()
    try:
        yield loop
    finally:
        loop.close()


@pytest.fixture(scope="session")
def file_export_filter() -> Generator[None, None, None]:
    with mock.patch.dict(
        os.environ,
        {
            "RUST_LOG": "error,pyo3_opentelemetry_lib=info",
        },
    ):
        yield


@pytest.fixture(scope="session")
async def otlp_service_data() -> AsyncGenerator[MutableMapping[str, List[ResourceSpans]], None]:
    manager = mp.Manager()
    data = manager.dict()
    sock = socket.socket()
    sock.bind(("", 0))
    address = f"localhost:{sock.getsockname()[1]}"
    process = mp.Process(
        target=_start_otlp_service,
        args=(
            data,
            sock.getsockname()[1],
        ),
    )
    process.start()

    try:
        # wait for the port to open
        async with insecure_channel(address) as channel:
            await asyncio.wait_for(channel.channel_ready(), timeout=30)

        with mock.patch.dict(
            os.environ,
            {
                "OTEL_EXPORTER_OTLP_ENDPOINT": f"http://{address}",
                "OTEL_EXPORTER_OTLP_INSECURE": "true",
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT": f"http://{address}",
                "OTEL_EXPORTER_OTLP_HEADERS": ",".join([f"{k}={v}" for k, v in _SERVICE_TEST_HEADERS.items()]),
                "OTEL_EXPORTER_OTLP_TIMEOUT": "1s",
                "RUST_LOG": "error,pyo3_opentelemetry_lib=info",
            },
        ):
            yield data
    finally:
        process.kill()


@pytest.fixture(scope="function")
def otlp_test_namespace() -> Generator[str, None, None]:
    namespace = str(uuid4())
    env = os.environ.copy()
    env["OTEL_EXPORTER_OTLP_HEADERS"] += f",x-test-namespace={namespace}"
    with mock.patch.dict(os.environ, env):
        yield namespace
