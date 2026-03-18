FROM python:3.12-slim

# System dependencies for Playwright Chromium and common build tools.
# Installed first so they are cached in their own layer.
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Chromium runtime deps (from playwright install --with-deps)
    libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 \
    libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
    libgbm1 libpango-1.0-0 libcairo2 libasound2 libatspi2.0-0 \
    libwayland-client0 \
    # General useful system packages
    curl wget git jq poppler-utils \
    && rm -rf /var/lib/apt/lists/*

# Pre-install common Python libraries so agents don't need to pip-install
# them on every conversation, shaving ~15-30s off first runs.
RUN pip install --no-cache-dir \
    # Document generation
    openpyxl \
    python-pptx \
    python-docx \
    # Data & visualization
    Pillow \
    matplotlib \
    pandas \
    numpy \
    # Web / HTTP
    requests \
    httpx \
    beautifulsoup4 \
    lxml \
    # Browser automation
    playwright \
    && rm -rf /root/.cache

# Install Playwright Chromium browser binaries into a fixed path.
# This avoids the agent having to download ~150MB on every conversation.
ENV PLAYWRIGHT_BROWSERS_PATH=/usr/lib/playwright
RUN playwright install chromium \
    && chmod -R 755 /usr/lib/playwright

# Browser automation server + client scripts
COPY browser_server.py browser_client.py /usr/lib/clawkson/

# Create workspace directory structure
RUN mkdir -p /workspace/inputs /workspace/outputs \
    && chmod -R 777 /workspace

WORKDIR /workspace

CMD ["sleep", "infinity"]
