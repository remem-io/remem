import os
import urllib.request
import json

def get_key():
    with open('.env', 'r') as f:
        for line in f:
            if line.startswith('GOOGLE_API_KEY='):
                return line.split('=', 1)[1].strip().strip('"').strip("'")
    return os.environ.get('GOOGLE_API_KEY')

key = get_key()

url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-2:embedContent?key={key}"
data = json.dumps({
    "model": "models/gemini-embedding-2",
    "content": {"parts": [{"text": "Hello world"}]}
}).encode('utf-8')

req = urllib.request.Request(url, data=data, headers={'Content-Type': 'application/json'})

try:
    with urllib.request.urlopen(req) as response:
        resp = json.loads(response.read().decode('utf-8'))
        emb = resp.get('embedding', {}).get('values', [])
        print(f"Dimension: {len(emb)}")
except Exception as e:
    print(f"Error: {e}")
