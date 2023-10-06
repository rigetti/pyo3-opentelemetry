from {{ host_package }}.{{ tracing_subscriber_module_name }}.layers import otlp


__doc__ = otlp.__doc__
__all__ = getattr(otlp, "__all__", [])

