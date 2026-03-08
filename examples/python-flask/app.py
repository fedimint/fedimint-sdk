from flask import Flask, render_template, jsonify
import os

app = Flask(__name__)

# Ensure .wasm files are served with the correct MIME type
app.config['SEND_FILE_MAX_AGE_DEFAULT'] = 0

@app.after_request
def add_headers(response):
    response.headers["Cross-Origin-Opener-Policy"] = "same-origin"
    response.headers["Cross-Origin-Embedder-Policy"] = "require-corp"
    response.headers["Cross-Origin-Resource-Policy"] = "cross-origin"
    # Serve wasm with correct MIME type
    if response.content_type == "application/wasm":
        response.headers["Content-Type"] = "application/wasm"
    return response

@app.route("/")
def index():
    return render_template("index.html")

@app.route("/api/health")
def health():
    return jsonify({"status": "ok", "message": "Flask server is running"})

@app.route("/api/invite-code")
def invite_code():
    code = os.environ.get(
        "FEDIMINT_INVITE_CODE",
        "fed11qgqzxgthwden5te0v9cxjtnzd96xxmmfdckhqunfde3kjurvv4ejucm0d5hsqqfqkggx3jz0tvfv5n7lj0e7gs7nh47z06ry95x4963wfh8xlka7a80su3952t"
    )
    return jsonify({"inviteCode": code})

if __name__ == "__main__":
    port = int(os.environ.get("PORT", 3000))
    print(f"Fedimint Flask example running at http://localhost:{port}")
    print("   Open the URL and check your browser console.")
    app.run(host="0.0.0.0", port=port, debug=True)