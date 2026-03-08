---
aside: false
---

# Python (Flask) Example

This example shows how to use the `@fedimint/core` package in a Python Flask application.

The Fedimint SDK runs entirely in the browser via WebAssembly — Flask simply serves the HTML page. There's minimal on-screen UI, so open your browser's console to see the library in action.

[Code on Github](https://github.com/fedimint/fedimint-sdk/tree/main/examples/python-flask)

## Running the Example Locally

Clone the repo:

```sh
git clone https://github.com/fedimint/fedimint-sdk.git
cd fedimint-sdk/examples/python-flask
```

Optional: create a virtual environment:

```sh
python -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
```

Install dependencies and download the SDK:

```sh
pip install -r requirements.txt
python setup_sdk.py
```

Run the example:

```sh
python app.py
```

Open `http://localhost:3000` on your browser to see the result. Check the **Console** tab (`F12`) to see:

```
Fedimint wallet initialized FedimintWallet { … }
balance 0
```

## Environment variables

| Variable               | Default   | Description                 |
| ---------------------- | --------- | --------------------------- |
| `PORT`                 | `3000`    | Server port                 |
| `FEDIMINT_INVITE_CODE` | demo code | Your federation invite code |
