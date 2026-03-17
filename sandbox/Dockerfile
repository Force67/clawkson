FROM python:3.12-slim

# Pre-install common document-generation libraries so agents don't need to
# pip-install them on every conversation, shaving ~15-30s off first runs.
RUN pip install --no-cache-dir \
    openpyxl \
    python-pptx \
    python-docx \
    Pillow \
    matplotlib \
    pandas \
    && rm -rf /root/.cache

# Create workspace directory structure
RUN mkdir -p /workspace/inputs /workspace/outputs \
    && chmod -R 777 /workspace

WORKDIR /workspace

CMD ["sleep", "infinity"]
