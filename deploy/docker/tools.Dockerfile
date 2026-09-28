# Provision / verify runner. Runs inside the compose network so that in rehearsal the
# lab hostnames resolve through the edge container's network aliases.
FROM python:3.12-slim
RUN pip install --no-cache-dir pillow==11.3.0
WORKDIR /lab
