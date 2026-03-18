#!/usr/bin/env python3
"""Browser client — sends a single command to the persistent browser server.

Usage: browser_client.py <action> [params_file.json]

Ensures the server is running before sending the command.
"""
import json, os, subprocess, sys, time, urllib.request

SERVER = "http://127.0.0.1:9222"
SERVER_SCRIPT = "/usr/lib/clawkson/browser_server.py"


def ensure_server():
    """Check if server is running; start it if not. Returns True on success."""
    for _ in range(2):
        try:
            urllib.request.urlopen(f"{SERVER}/health", timeout=2)
            return True
        except Exception:
            pass

    # Start server in background
    subprocess.Popen(
        [sys.executable, SERVER_SCRIPT],
        stdout=open("/tmp/browser_server.log", "w"),
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    # Wait for server to become ready (Chromium launch takes a few seconds)
    for _ in range(20):
        time.sleep(1)
        try:
            urllib.request.urlopen(f"{SERVER}/health", timeout=2)
            return True
        except Exception:
            pass
    return False


def main():
    action = sys.argv[1]

    # Read params from file if provided, otherwise empty object
    if len(sys.argv) > 2:
        params_path = sys.argv[2]
        with open(params_path) as f:
            params = json.load(f)
        # Clean up params file
        try:
            os.unlink(params_path)
        except OSError:
            pass
    else:
        params = {}

    if not ensure_server():
        # Dump server log for debugging
        log = ""
        try:
            with open("/tmp/browser_server.log") as f:
                log = f.read()[-500:]
        except Exception:
            pass
        print(json.dumps({"error": f"Browser server failed to start. Log: {log}"}))
        return

    try:
        data = json.dumps(params).encode()
        req = urllib.request.Request(
            f"{SERVER}/{action}",
            data=data,
            headers={"Content-Type": "application/json"},
        )
        resp = urllib.request.urlopen(req, timeout=60)
        result = json.loads(resp.read())
        # Add output_files for image streaming
        if "screenshot" in result:
            screenshot_path = result["screenshot"]
            try:
                size = os.path.getsize(f"/workspace/{screenshot_path}")
            except OSError:
                size = 0
            result["output_files"] = [{"path": screenshot_path, "size": size}]
        print(json.dumps(result, default=str))
    except Exception as e:
        print(json.dumps({"error": str(e)}))


if __name__ == "__main__":
    main()
