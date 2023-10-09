from typing import Optional


class Config:
    def __init__(self, *, file_path: Optional[str] = None, pretty: bool = False, filter: Optional[str], json: bool = True) -> None:
        ...


