# Digital Identity Card - ONVM WASM Program

A blockchain-based digital identity system that stores user profiles and generates dynamic SVG profile cards.

## Features

- **Identity Registration**: Create unique blockchain-based identities
- **Profile Management**: Store name, email, bio, avatar, and theme preferences
- **Dynamic SVG Cards**: Auto-generated profile cards with customizable themes
- **Data Integrity**: Blake3 checksums for all identities
- **Persistent Storage**: Uses ONVM state management for blockchain storage

## Operations

### 1. Register Identity
```json
{"op":"register","name":"Alice Smith","email":"alice@example.com","bio":"Blockchain developer","theme":"blue"}
```

**Response:**
```json
{
  "status": "okregister",
  "id": "a1b2c3d4e5f6...",
  "identity": {
    "id": "a1b2c3d4e5f6...",
    "name": "Alice Smith",
    "email": "alice@example.com",
    "bio": "Blockchain developer",
    "avatar_url": null,
    "theme": "blue",
    "created_at": 1702650000,
    "updated_at": 1702650000,
    "checksum": "blake3_hash..."
  },
  "card_svg": "<svg>...</svg>"
}
```

### 2. Get Profile
```json
{"op":"getprofile","id":"a1b2c3d4e5f6..."}
```

### 3. Update Profile
```json
{"op":"updateprofile","id":"a1b2c3d4e5f6...","bio":"New bio","theme":"purple"}
```

### 4. Get Profile Card
```json
{"op":"getcard","id":"a1b2c3d4e5f6...","format":"svg"}
```

Returns dynamic SVG profile card with identity information.

### 5. List All Identities
```json
{"op":"listall"}
```

### 6. Get Statistics
```json
{"op":"stats"}
```

Returns total identities, storage usage, and merkle root.

## Available Themes

- `blue` (default) - Professional blue gradient
- `purple` - Creative purple gradient
- `green` - Nature green gradient
- `red` - Bold red gradient
- `orange` - Warm orange gradient
- `pink` - Vibrant pink gradient
- `dark` - Dark mode theme
- `light` - Light mode theme

## SVG Card Features

- **Avatar Support**: Display custom avatar URLs or auto-generated initials
- **Theme Customization**: 8 pre-built color schemes
- **Gradient Backgrounds**: Beautiful gradient fills
- **Identity Badge**: Visual checkmark indicator
- **Responsive Text**: Auto-truncation for long bio text
- **Timestamp Display**: Human-readable creation dates
- **Checksum Display**: First 12 chars of blake3 hash
- **ID Shortening**: Compact ID display

## Building

```bash
cd wasm_programs/identity_card
cargo build --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/onvm_identity_card.wasm`

## Usage with ONVM

1. Upload the WASM program:
```bash
onvm upload-blob --file target/wasm32-unknown-unknown/release/onvm_identity_card.wasm --rpc 127.0.0.1:8080
```

2. Deploy program:
```bash
onvm deploy --blob-id blob<64-hex> --rpc 127.0.0.1:8080
```

3. Execute operations:
```bash
onvm execute --program-id prog<64-hex> --input register.json --rpc 127.0.0.1:8080
```

## Example Workflow

```bash
# 1. Register Alice
onvm execute --program-id prog<64-hex> --input register.json --rpc 127.0.0.1:8080

# 2. Register Bob
onvm execute --program-id prog<64-hex> --input register2.json --rpc 127.0.0.1:8080

# 3. List all identities
onvm execute --program-id prog<64-hex> --input list_all.json --rpc 127.0.0.1:8080

# 4. Get Alice's card (use ID from step 1)
# Edit get_card.json with actual ID, then:
onvm execute --program-id prog<64-hex> --input get_card.json --rpc 127.0.0.1:8080

# 5. Save SVG to file
onvm execute --program-id prog<64-hex> --input get_card.json --rpc 127.0.0.1:8080 | jq -r '.card_svg' > alice_card.svg
```

## Data Model

### Identity
```rust
{
  id: String,           // 16-byte hex (blake3 hash)
  name: String,         // 1-100 chars
  email: String,        // Valid email, max 320 chars
  bio: Option<String>,  // Optional biography
  avatar_url: Option<String>, // Optional avatar image URL
  theme: String,        // Color theme name
  created_at: u64,      // Unix timestamp
  updated_at: u64,      // Unix timestamp
  checksum: String      // Blake3 hash of identity data
}
```

## Security Features

- Email validation (format + length)
- Name validation (1-100 chars)
- Duplicate identity prevention
- XML escaping for SVG output
- Input sanitization
- Checksum verification

## Storage Layout

- `identity:<id>` → JSON-encoded Identity
- `__identity_index` → JSON array of all identity IDs

## Error Handling

All errors return:
```json
{"status":"err","message":"error description"}
```

Common errors:
- `invalid email format`
- `name must be 1-100 characters`
- `identity already exists`
- `key not found`
- `invalid json`
