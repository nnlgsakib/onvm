# nbook - Modern Social Media Platform

A stunning, production-ready social media frontend built with Next.js 16, featuring a sleek black and blue design with extensive micro-interactions and animations.

## Features

### Landing Page
- Animated hero section with gradient text and glowing effects
- Interactive feature cards with hover animations
- Live statistics counter
- Smooth scroll animations and transitions
- Fully responsive mobile design

### Authentication
- Beautiful login and register pages
- Real-time password validation
- Glassmorphic card designs
- Animated form interactions
- Session management utilities

### Feed Interface
- Three-column layout (sidebar, feed, right panel)
- Real-time post creation with character counter
- Interactive post cards with like, comment, share, and bookmark
- Trending topics sidebar
- User suggestions
- Mobile-optimized navigation
- Smooth transitions and hover effects

### Profile Pages
- Customizable cover photos and avatars
- User bio and metadata
- Follower/following statistics
- Tabbed content (posts, replies, media, likes)
- Edit profile functionality

### Discover
- Trending topics with growth indicators
- Suggested users to follow
- Discovery feed with popular posts
- Topic-based browsing

### Notifications
- Real-time notification feed
- Categorized by type (likes, comments, follows)
- Read/unread indicators
- Interactive notification cards

### Post Creation & Interactions
- Full-featured post creation page
- Image upload support
- Character limit tracking
- Detailed post view with comments
- Nested comment threads
- Real-time interaction updates

## Design System

### Color Palette
- **Primary**: Blue (oklch(0.62 0.22 240))
- **Accent**: Light Blue (oklch(0.55 0.25 220))
- **Background**: Dark (oklch(0.12 0.01 240))
- **Neutrals**: Various shades of dark blue-grays

### Typography
- **Font Family**: Geist (sans-serif) and Geist Mono (monospace)
- **Scale**: Responsive text sizing with mobile-first approach

### Animations & Interactions
- Smooth hover effects on all interactive elements
- Scale transforms on buttons and cards
- Color transitions on navigation items
- Floating animations for hero elements
- Pulse effects for notifications
- Gradient shimmer effects

## Tech Stack

- **Framework**: Next.js 16 (App Router)
- **Styling**: Tailwind CSS v4
- **UI Components**: shadcn/ui
- **Icons**: Lucide React
- **Animations**: CSS transitions and keyframes
- **TypeScript**: Full type safety

## Project Structure

```
app/
├── page.tsx                    # Landing page
├── login/page.tsx             # Login page
├── register/page.tsx          # Register page
├── feed/page.tsx              # Main feed
├── profile/page.tsx           # User profile
├── discover/page.tsx          # Discovery page
├── notifications/page.tsx     # Notifications
├── create/page.tsx            # Create post
├── post/[id]/page.tsx        # Post detail
└── layout.tsx                 # Root layout

components/
├── landing/                   # Landing page components
├── feed/                      # Feed components
├── profile/                   # Profile components
├── discover/                  # Discovery components
├── notifications/             # Notification components
├── create/                    # Post creation components
├── post/                      # Post detail components
└── ui/                        # Reusable UI components

lib/
├── auth.ts                    # Authentication utilities
├── api.ts                     # API integration utilities
└── utils.ts                   # General utilities
```

## Integration with Your WASM Backend

The frontend is designed to be integrated with your Rust WASM backend. Here's how to connect them:

### 1. API Integration

Update the `lib/api.ts` file to point to your WASM backend:

```typescript
const API_BASE_URL = "YOUR_WASM_API_URL"
```

### 2. Authentication Flow

The authentication utilities in `lib/auth.ts` store session tokens in localStorage. Integrate with your backend's authentication:

```typescript
// On successful login/register
setSession(response.session, username, response.expires_ms)

// Check authentication status
const isLoggedIn = isAuthenticated()
```

### 3. API Request Format

All API functions follow your WASM backend's request format:

```typescript
{
  op: "operation_name",
  session: "session_token",
  // ...other parameters
}
```

### 4. Available API Functions

- `createPost(text, attachments)`
- `likePost(postId)`
- `commentOnPost(postId, text)`
- `followUser(username)`
- `getFeed(limit, discover)`
- `getProfile(username)`

## Customization

### Colors

Update the color scheme in `app/globals.css` by modifying the CSS custom properties:

```css
:root {
  --primary: oklch(0.62 0.22 240);
  --accent: oklch(0.55 0.25 220);
  /* ...other colors */
}
```

### Animations

All animations are defined in `app/globals.css`. You can modify existing animations or add new ones:

```css
@keyframes your-animation {
  0% { /* initial state */ }
  100% { /* final state */ }
}
```

## Responsive Design

The entire frontend is fully responsive with breakpoints:
- **Mobile**: < 768px
- **Tablet**: 768px - 1024px
- **Desktop**: > 1024px
- **Large Desktop**: > 1280px

## Performance Optimizations

- Server-side rendering for initial page loads
- Lazy loading of images and components
- Optimized animations using CSS transforms
- Minimal JavaScript bundle size
- Edge-optimized with Next.js

## Browser Support

- Chrome (latest)
- Firefox (latest)
- Safari (latest)
- Edge (latest)

## Getting Started

1. Install dependencies:
```bash
npm install
```

2. Run the development server:
```bash
npm run dev
```

3. Open [http://localhost:3000](http://localhost:3000)

## Production Build

```bash
npm run build
npm start
```

## License

MIT License - feel free to use this frontend for your projects!

---

Built with passion for great user experiences.
