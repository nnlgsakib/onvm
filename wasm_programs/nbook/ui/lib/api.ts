import { getNbookClient } from "./onvm-client"
import { getSession } from "./auth"
import type { Post, ProfileView, UserSummary } from "./onvm-client"

function getSessionToken(): string | null {
  const session = getSession()
  return session?.session || null
}

// Post operations
export async function createPost(text: string, attachments: string[] = []): Promise<Post> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.createPost(sessionToken, text, attachments)
}

export async function likePost(postId: string): Promise<Post> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.likePost(sessionToken, postId)
}

export async function commentOnPost(postId: string, text: string): Promise<Post> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.commentOnPost(sessionToken, postId, text)
}

export async function getPost(postId: string): Promise<Post> {
  const client = getNbookClient()
  return await client.getPost(postId)
}

// Social operations
export async function followUser(username: string): Promise<void> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.follow(sessionToken, username)
}

export async function unfollowUser(username: string): Promise<void> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.unfollow(sessionToken, username)
}

// Feed operations
export async function getFeed(limit = 20, discover = false): Promise<Post[]> {
  const client = getNbookClient()
  const sessionToken = getSessionToken()

  if (!sessionToken) {
    throw new Error("Not authenticated")
  }

  return await client.getFeed(sessionToken, limit, discover)
}

// Profile operations
export async function getProfile(username: string): Promise<ProfileView> {
  const client = getNbookClient()
  return await client.getProfile(username)
}

export async function updateProfile(
  displayName?: string,
  bio?: string,
  avatarBlobId?: string,
  bannerBlobId?: string
): Promise<void> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.updateProfile(session.session, displayName, bio, avatarBlobId, bannerBlobId)
}

export async function deletePost(postId: string): Promise<void> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.deletePost(session.session, postId)
}

export async function searchUsers(query: string, limit = 10): Promise<UserSummary[]> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.searchUsers(session.session, query, limit)
}

export async function getSuggestions(limit = 5): Promise<UserSummary[]> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.getSuggestions(session.session, limit)
}

export async function listFollowers(username: string, limit = 50): Promise<UserSummary[]> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.listFollowers(session.session, username, limit)
}

export async function listFollowing(username: string, limit = 50): Promise<UserSummary[]> {
  const client = getNbookClient()
  const session = getSession()
  if (!session?.session) {
    throw new Error("Not authenticated")
  }
  return client.listFollowing(session.session, username, limit)
}

// Blob operations
export async function uploadBlob(file: File): Promise<string> {
  const client = getNbookClient()
  const buffer = await file.arrayBuffer()
  return await client.uploadBlob(buffer)
}

export async function fetchBlob(id: string): Promise<{ blob: Blob; contentType: string }> {
  const client = getNbookClient()
  return client.fetchBlob(id)
}
