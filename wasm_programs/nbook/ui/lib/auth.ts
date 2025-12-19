import { getNbookClient } from "./onvm-client"

export interface User {
  username: string
  session: string
  expiresAt: number
}

export async function register(username: string, password: string): Promise<void> {
  const client = getNbookClient()
  await client.register(username, password)
}

export async function login(username: string, password: string): Promise<User> {
  const client = getNbookClient()
  const { session, expiresMs } = await client.login(username, password)

  const user: User = {
    username,
    session,
    expiresAt: expiresMs,
  }

  setSession(session, username, expiresMs)
  return user
}

export async function logout(): Promise<void> {
  const session = getSession()
  if (session) {
    const client = getNbookClient()
    try {
      await client.logout(session.session)
    } catch (error) {
      console.error("[v0] Logout error:", error)
    }
    clearSession()
  }
}

export function setSession(session: string, username: string, expiresMs: number) {
  if (typeof window !== "undefined") {
    const user: User = {
      username,
      session,
      expiresAt: expiresMs,
    }
    localStorage.setItem("nbook_session", JSON.stringify(user))
  }
}

export function getSession(): User | null {
  if (typeof window !== "undefined") {
    const data = localStorage.getItem("nbook_session")
    if (data) {
      const user: User = JSON.parse(data)
      if (user.expiresAt > Date.now()) {
        return user
      } else {
        clearSession()
      }
    }
  }
  return null
}

export function clearSession() {
  if (typeof window !== "undefined") {
    localStorage.removeItem("nbook_session")
  }
}

export function isAuthenticated(): boolean {
  return getSession() !== null
}

export function getCurrentUsername(): string | null {
  const session = getSession()
  return session?.username || null
}
