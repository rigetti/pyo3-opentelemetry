FROM rust:latest

RUN apt install -y curl git

ENV PYENV_ROOT="/usr/local/pyenv"
RUN curl https://pyenv.run | bash
ENV PATH="${PYENV_ROOT}/bin:${PATH}"

RUN eval "$(pyenv init -)"
RUN pyenv install 3.8.16
RUN pyenv install 3.9.16
RUN pyenv install 3.10.11
RUN pyenv install 3.11.2

RUN pyenv global 3.11.2

ENV POETRY_VERSION=1.4.2
ENV POETRY_HOME="/usr/local/poetry"
ENV PATH="${POETRY_HOME}/bin:${PATH}"
RUN curl -sSL https://install.python-poetry.org | python3 -

RUN cargo install cargo-make cargo-hack grcov cargo-deny cargo-license cargo-msrv cargo-outdated cargo-nextest cargo-deadlinks 
RUN rustup component add clippy llvm-tools-preview

