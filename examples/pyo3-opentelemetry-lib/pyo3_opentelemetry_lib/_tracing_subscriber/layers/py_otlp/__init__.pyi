from typing import List, Optional, Protocol


class OtlpBytesExporter(Protocol):
    def export(self, serialized_resource_spans: List[bytes]) -> None:
        ...


class Config:
    def __init__(self, *, exporter: OtlpBytesExporter) -> None:
        ...
