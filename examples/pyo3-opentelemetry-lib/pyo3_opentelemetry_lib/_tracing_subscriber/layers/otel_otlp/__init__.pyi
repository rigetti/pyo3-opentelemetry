from typing import Dict, List, Optional, Union

class SpanLimits:
    def __init__(
        self,
        *,
        max_events_per_span: Optional[int] = None,
        max_attributes_per_span: Optional[int] = None,
        max_links_per_span: Optional[int] = None,
        max_attributes_per_event: Optional[int] = None,
        max_attributes_per_link: Optional[int] = None,
    ) -> None: ...
    """

    :param max_events_per_span: The max events that can be added to a `Span`.
    :param max_attributes_per_span: The max attributes that can be added to a `Span`.
    :param max_links_per_span: The max links that can be added to a `Span`.
    :param max_attributes_per_event: The max attributes that can be added to an `Event`.
    :param max_attributes_per_link: The max attributes that can be added to a `Link`.
    """

ResourceValueArray = Union[List[bool], List[int], List[float], List[str]]
ResourceValue = Union[bool, int, float, str, ResourceValueArray]

class Resource:
    def __init__(
        self,
        *,
        attrs: Optional[Dict[str, ResourceValue]] = None,
        schema_url: Optional[str] = None,
    ) -> None: ...

Sampler = Union[bool, float]

class Config:
    def __init__(
        self,
        *,
        span_limits: Optional[SpanLimits] = None,
        resource: Optional[Resource] = None,
        metadata_map: Optional[Dict[str, str]] = None,
        sampler: Optional[Sampler] = None,
        endpoint: Optional[str] = None,
        timeout_millis: Optional[int] = None,
    ) -> None: ...
