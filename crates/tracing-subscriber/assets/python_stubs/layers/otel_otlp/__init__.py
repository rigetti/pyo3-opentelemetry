from {{ host_package }}.{{ tracing_subscriber_module_name }}.layers import otel_otlp


__doc__ = otel_otlp.__doc__
__all__ = getattr(otel_otlp, "__all__", [])

