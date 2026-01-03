# Identity Card UI - ONVM Digital Identity Application

A web interface for the ONVM Digital Identity Card WASM program that allows users to:

- Register new digital identities with name, email, bio, avatar, and theme
- View identity gallery with all registered identities
- See real-time system statistics
- Generate and display SVG identity cards

## Features

- **Modern UI**: Built with Next.js 16 and React 19
- **Responsive Design**: Works on desktop and mobile devices
- **Real-time Data**: Connects directly to ONVM RPC endpoints
- **Theme Customization**: 8 color themes for identity cards
- **SVG Generation**: Dynamic SVG identity card generation
- **TypeScript**: Full TypeScript support for type safety

## Getting Started

### Prerequisites

- Node.js 18 or later
- pnpm (recommended) or npm
- A running ONVM node with RPC endpoint

### Installation

1. Install dependencies:
```bash
pnpm install
```

2. Copy the environment example file and configure it:
```bash
cp .env.example .env.local
```

3. Edit `.env.local` and set your ONVM RPC endpoint:
```bash
NEXT_PUBLIC_ONVM_RPC_URL=http://localhost:8080
NEXT_PUBLIC_IDENTITY_CARD_PROGRAM_ID=prog<64-hex>
```

### Running the Development Server

```bash
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the application.

### Building for Production

```bash
pnpm build
```

### Running in Production

```bash
pnpm start
```

## Project Structure

- `app/` - Next.js app router pages
- `components/` - React components
- `lib/` - Utility functions and ONVM client
- `public/` - Static assets
- `styles/` - Global styles

## ONVM Integration

The UI connects to ONVM through the RPC client in `lib/onvm-client.ts` which provides methods for:

- Registering identities
- Retrieving identity profiles
- Updating identity profiles
- Generating identity cards
- Listing all identities
- Getting system statistics

## Environment Variables

- `NEXT_PUBLIC_ONVM_RPC_URL` - The ONVM RPC endpoint URL
- `NEXT_PUBLIC_IDENTITY_CARD_PROGRAM_ID` - The deployed WASM program ID (prog<64-hex>)

## Learn More

To learn more about the technologies used:

- [Next.js Documentation](https://nextjs.org/docs)
- [React Documentation](https://react.dev/)
- [ONVM Documentation](https://github.com/your-org/onvm)
