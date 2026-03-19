# Resty Form Alpha

Lightweight Telegram Web App for employee evaluations, built with Dioxus (Rust → WASM).

## Features

- **Ultra-lightweight**: Optimized WASM binary (~150-200KB gzipped)
- **Stateless auth**: ChaCha20-encrypted tokens (no session storage needed)
- **Multiple question types**: Boolean (Yes/No), Number, Text
- **Validation**: Required fields, instant feedback
- **Mobile-first**: Responsive design optimized for Telegram

## Architecture

### Authentication Flow

```
1. Bot → API: POST /api/webapp/token (create encrypted token)
2. API → Bot: {token: "encrypted_payload", url: "https://...?token=xxx"}
3. Bot → User: Web App button with URL
4. User → Web App: Opens form
5. Web App → API: GET /form/{token} (token decrypted, data fetched)
6. Web App → API: POST /form/{token}/submit (answers saved)
7. Web App: Closes automatically
```

Token contains encrypted:
- `org_id`: Organization ID
- `set_id`: Criterion set ID
- `filler_id`: Who fills the form
- `eval_id`: Who is being evaluated (optional)
- `type_id`: Evaluation type
- `exp`: Expiration timestamp
- `nonce`: Random for uniqueness

### Security

- **ChaCha20Poly1305**: Authenticated encryption (AEAD)
- **URL-safe Base64**: Token in URL query parameter
- **Expiration**: Tokens expire after configurable time (default 1 hour)
- **Replay protection**: Used tokens are tracked

## Project Structure

```
resty_form_alpha/
├── src/
│   ├── main.rs          # Entry point
│   ├── telegram.rs      # Telegram Web App UX helpers
│   ├── types.rs         # Data types
│   ├── api.rs           # HTTP client
│   └── components/
│       ├── mod.rs
│       ├── form.rs      # Main form component
│       ├── question.rs  # Question card
│       └── inputs.rs    # Input components
├── assets/
│   └── styling/
│       └── main.css     # Styles
├── Cargo.toml
└── Dioxus.toml
```

## Development

### Prerequisites

- Rust (latest stable)
- Dioxus CLI: `cargo install dioxus-cli`

### Run locally

```bash
dx serve --platform web
```

Opens at http://localhost:8080

### Build for production

```bash
dx build --release --platform web
```

Output in `dist/` folder.

## Integration with Bot

### 1. Generate secret key

```bash
python -c "import secrets; print(secrets.token_hex(32))"
```

Add to `.env`:

```env
WEBAPP_SECRET_KEY=your_64_character_hex_key_here
WEBAPP_BASE_URL=https://your-domain.com
WEBAPP_TOKEN_EXPIRY=3600
```

### 2. Start the webapp (dev)

```bash
cd resty_form_alpha
dx serve --platform web
```

### 3. Start ngrok (for HTTPS in dev)

```bash
ngrok http 8080
```

Update `WEBAPP_BASE_URL` with ngrok URL.

### 4. Bot creates form token

```python
# In bot handler, call API to create token:
import httpx

async with httpx.AsyncClient() as client:
    response = await client.post(
        "http://localhost:8000/api/webapp/token",
        json={
            "organization_id": 1,
            "criterion_set_id": 1,
            "filled_by_employee_id": 1,
            "evaluated_employee_id": 2,
        }
    )
    data = response.json()
    # data = {"token": "xxx", "url": "https://...?token=xxx"}
```

### 5. Bot sends Web App button

```python
from aiogram.types import WebAppInfo, InlineKeyboardButton, InlineKeyboardMarkup

keyboard = InlineKeyboardMarkup(inline_keyboard=[[
    InlineKeyboardButton(
        text="📝 Заполнить форму",
        web_app=WebAppInfo(url=data["url"])
    )
]])
await message.answer("Нажмите кнопку:", reply_markup=keyboard)
```

## API Endpoints

### POST /api/webapp/token
Creates encrypted form token (called by bot).

**Request:**
```json
{
  "organization_id": 1,
  "criterion_set_id": 1,
  "filled_by_employee_id": 1,
  "evaluated_employee_id": 2,
  "evaluation_type_id": 1
}
```

**Response:**
```json
{
  "token": "base64url_encrypted_payload",
  "url": "https://webapp.example.com?token=..."
}
```

### GET /api/webapp/form/{token}
Returns form data (criteria list). Token is decrypted and validated.

### POST /api/webapp/form/{token}/submit
Submits form answers. Token is validated and marked as used.

## Optimization Notes

### WASM Size Reduction
- `opt-level = "z"` - Optimize for size
- `lto = true` - Link-time optimization
- `panic = "abort"` - No unwinding
- `strip = true` - Strip symbols
- Minimal dependencies

### Performance
- Single-page app (no routing overhead)
- Signals for efficient re-renders
- CSS variables for theming (no JS)
- Lazy comment expansion
- Stateless auth (no session storage)

## Theme IntegrationThe app adapts to Telegram theme via CSS variables:
- `--tg-theme-bg-color`
- `--tg-theme-text-color`
- `--tg-theme-button-color`Dark mode supported via `prefers-color-scheme`.
