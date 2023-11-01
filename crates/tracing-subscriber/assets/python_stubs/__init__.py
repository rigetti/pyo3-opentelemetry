from {{ host_package }} import {{ tracing_subscriber_module_name }}


__doc__ = {{ tracing_subscriber_module_name }}.__doc__
__all__ = getattr({{ tracing_subscriber_module_name }}, "__all__", [])
