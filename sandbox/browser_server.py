#!/usr/bin/env python3
"""Persistent browser session server for Clawkson sandbox agents.

Manages a single Playwright Chromium browser that persists across tool calls.
Accepts commands via a simple HTTP API on 127.0.0.1:9222.
"""
import json, os, sys, traceback
from http.server import HTTPServer, BaseHTTPRequestHandler
from playwright.sync_api import sync_playwright


class BrowserSession:
    def __init__(self):
        self.pw = sync_playwright().start()
        self.browser = self.pw.chromium.launch(
            headless=True,
            args=[
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--disable-software-rasterizer",
            ],
        )
        self.page = self.browser.new_page(viewport={"width": 1280, "height": 720})

    def _page_info(self):
        """Capture current page state for the model."""
        os.makedirs("/workspace/outputs", exist_ok=True)
        self.page.screenshot(path="/workspace/outputs/browser_screenshot.png")

        try:
            elements = self.page.evaluate(
                """() => {
                const r = {links:[], buttons:[], inputs:[], text:''};

                // Clickable links
                for (const a of document.querySelectorAll('a[href]')) {
                    const t = a.textContent?.trim();
                    if (t && t.length > 0 && t.length < 100) {
                        r.links.push({text: t, href: a.href});
                    }
                }
                r.links = r.links.slice(0, 25);

                // Buttons
                for (const b of document.querySelectorAll('button, [role="button"], input[type="submit"], input[type="button"]')) {
                    const t = (b.textContent?.trim() || b.value || b.getAttribute('aria-label') || '').substring(0, 80);
                    if (t) {
                        const sel = b.id ? '#'+b.id : (b.name ? '[name=\"'+b.name+'\"]' : null);
                        r.buttons.push({text: t, selector: sel});
                    }
                }
                r.buttons = r.buttons.slice(0, 15);

                // Input fields
                for (const el of document.querySelectorAll('input:not([type="hidden"]), textarea, select')) {
                    r.inputs.push({
                        tag: el.tagName.toLowerCase(),
                        type: el.type || null,
                        name: el.name || null,
                        id: el.id || null,
                        placeholder: el.placeholder || null,
                        aria_label: el.getAttribute('aria-label') || null,
                    });
                }
                r.inputs = r.inputs.slice(0, 15);

                // Visible text (truncated)
                r.text = (document.body?.innerText || '').substring(0, 3000);
                return r;
            }"""
            )
        except Exception:
            elements = {"text": "", "links": [], "buttons": [], "inputs": []}

        return {
            "title": self.page.title(),
            "url": self.page.url,
            "screenshot": "outputs/browser_screenshot.png",
            "elements": elements,
        }

    def navigate(self, url, wait_until="load"):
        try:
            self.page.goto(url, wait_until=wait_until, timeout=30000)
        except Exception as e:
            if "timeout" not in str(e).lower():
                raise
        return self._page_info()

    def click(self, selector):
        self.page.click(selector, timeout=5000)
        try:
            self.page.wait_for_load_state("networkidle", timeout=5000)
        except Exception:
            pass
        return self._page_info()

    def type_text(self, selector, text):
        self.page.fill(selector, text, timeout=5000)
        return self._page_info()

    def screenshot(self):
        return self._page_info()

    def evaluate(self, expression):
        result = self.page.evaluate(expression)
        info = self._page_info()
        info["eval_result"] = result
        return info

    def scroll(self, direction="down", amount=500):
        delta = amount if direction == "down" else -amount
        self.page.mouse.wheel(0, delta)
        self.page.wait_for_timeout(500)
        return self._page_info()

    def back(self):
        self.page.go_back()
        try:
            self.page.wait_for_load_state("load", timeout=5000)
        except Exception:
            pass
        return self._page_info()


session = None


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok")
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length)) if length else {}
        action = self.path.strip("/")

        try:
            if action == "navigate":
                result = session.navigate(
                    body["url"], body.get("wait_until", "load")
                )
            elif action == "click":
                result = session.click(body["selector"])
            elif action == "type":
                result = session.type_text(body["selector"], body["text"])
            elif action == "screenshot":
                result = session.screenshot()
            elif action == "evaluate":
                result = session.evaluate(body["expression"])
            elif action == "scroll":
                result = session.scroll(
                    body.get("direction", "down"), body.get("amount", 500)
                )
            elif action == "back":
                result = session.back()
            else:
                self._respond(400, {"error": f"Unknown action: {action}"})
                return

            self._respond(200, result)
        except Exception as e:
            self._respond(500, {"error": str(e), "traceback": traceback.format_exc()})

    def _respond(self, code, data):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(data, default=str).encode())

    def log_message(self, *args):
        pass  # suppress access logs


if __name__ == "__main__":
    session = BrowserSession()
    print("Browser server ready on port 9222", flush=True)
    HTTPServer(("127.0.0.1", 9222), Handler).serve_forever()
