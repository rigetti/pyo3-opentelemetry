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
from contextlib import closing
import multiprocessing as mp
import os
import socket
from concurrent import futures
from time import sleep
from typing import AsyncGenerator, MutableSequence
from unittest import mock

import grpc
import pytest
from grpc.aio import ServicerContext, insecure_channel
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
    def __init__(self, data: MutableSequence[ResourceSpans]):
        self.lock = asyncio.Lock()
        self.resource_spans = data

    def _are_headers_set(self, context: ServicerContext) -> bool:
        metadata = context.invocation_metadata()
        if metadata is None:
            return False
        for k, v in _SERVICE_TEST_HEADERS.items():
            value = next((value for key, value in metadata if key == k), None)
            if value != v:
                return False
        return True

    async def Export(
        self, request: trace_service_pb2.ExportTraceServiceRequest, context: ServicerContext
    ) -> trace_service_pb2.ExportTraceServiceResponse:
        if not self._are_headers_set(context):
            context.set_code(grpc.StatusCode.PERMISSION_DENIED)
            return trace_service_pb2.ExportTraceServiceResponse()

        async with self.lock:
            self.resource_spans += list(request.resource_spans)
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


@pytest.fixture
async def otlp_service() -> AsyncGenerator[MutableSequence[ResourceSpans], None]:
    manager = mp.Manager()
    data = manager.list()
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
