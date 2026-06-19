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
if not key:
    print("No key found")
    exit(1)

url = f"https://generativelanguage.googleapis.com/v1beta/models?key={key}"
try:
    with urllib.request.urlopen(url) as response:
        data = json.loads(response.read().decode('utf-8'))
        for m in data.get('models', []):
            if 'embedContent' in m.get('supportedGenerationMethods', []):
                print(f"Supported embedding model: {m['name']}")
except Exception as e:
    print(f"Error: {e}")
