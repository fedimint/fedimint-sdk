# Python (Flask) Example – Fedimint SDK

This is a [Flask](https://flask.palletsprojects.com) example for the [Fedimint SDK](https://sdk.fedimint.org).

## Getting Started

First, run the development server:

```bash
pip install -r requirements.txt
python setup_sdk.py
python app.py
```

Open [https://localhost:3000](https://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `templates/index.html`. The server auto-reloads as you edit `app.py`.

## What you'll see

Open the browser console (`F12`) after loading the page:

```
Fedimint wallet initialized FedimintWallet { … }
balance 0
```

## Note

This example connects to the [mutinynet testnet federation](https://faucet.mutinynet.com).
If the federation is unreachable from your network, replace `FEDIMINT_INVITE_CODE` with
an invite code from a locally accessible federation found at [observer.fedimint.org](https://observer.fedimint.org).
