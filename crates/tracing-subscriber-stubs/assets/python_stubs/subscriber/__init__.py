from {{ host_package }}.{{ tracing_subscriber_module_name }} import subscriber


__doc__ = subscriber.__doc__
__all__ = getattr(subscriber, "__all__", [])

