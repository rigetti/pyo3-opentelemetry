from typing import Optional


class Config:
    def __init__(self, *, file_path: Optional[str] = None, filter: Optional[str] = None) -> None:
        ...

