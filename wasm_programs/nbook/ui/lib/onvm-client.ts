// ONVM Client for nbook Social Media WASM Program
// This client interacts with the ONVM RPC to manage social media operations

export interface User {
  id: string
  username: string
  email: string
  bio?: string
  avatar?: string
  coverImage?: string
  followers: number
  following: number
  posts: string[]
  createdAt: number
}

// API shapes (snake_case) returned by the WASM
interface ApiPost {
  id: string
  author: string
  text: string
  attachments: Attachment[]
  created_at: number
  likes: number
  comments: ApiCommentView[]
}

interface ApiCommentView {
  id: string
  author: string
  text: string
  created_at: number
}

export interface Attachment {
  blob_id_hex: string
  size: number
  hash_hex: string
}

// Client-facing camelCase shapes
export interface CommentView {
  id: string
  author: string
  text: string
  createdAt: number
}

export interface Post {
  id: string
  author: string
  text: string
  attachments: Attachment[]
  createdAt: number
  likes: number
  comments: CommentView[]
}

export interface ProfileView {
  username: string
  displayName?: string | null
  bio?: string | null
  avatarBlobId?: string | null
  bannerBlobId?: string | null
  followers: number
  following: number
  followerList: string[]
  followingList: string[]
  posts: string[]
  createdAt: number
}

export interface UserSummary {
  username: string
  displayName?: string | null
  bio?: string | null
  avatarBlobId?: string | null
  bannerBlobId?: string | null
  followers: number
}

interface RegisterRequest {
  op: "register"
  username: string
  password: string
}

interface LoginRequest {
  op: "login"
  username: string
  password: string
}

interface LogoutRequest {
  op: "logout"
  session: string
}

interface CreatePostRequest {
  op: "create_post"
  session: string
  text: string
  attachments: string[]
}

interface FollowRequest {
  op: "follow"
  session: string
  target: string
}

interface UnfollowRequest {
  op: "unfollow"
  session: string
  target: string
}

interface LikeRequest {
  op: "like"
  session: string
  post_id: string
}

interface CommentRequest {
  op: "comment"
  session: string
  post_id: string
  text: string
}

interface FeedRequest {
  op: "feed"
  session: string
  limit?: number
  discover?: boolean
}

interface GetPostRequest {
  op: "get_post"
  post_id: string
}

interface ProfileRequest {
  op: "profile"
  username: string
}

interface UpdateProfileRequest {
  op: "update_profile"
  session: string
  display_name?: string
  bio?: string
  avatar_blob_id?: string
  banner_blob_id?: string
}

interface SearchUsersRequest {
  op: "search_users"
  session: string
  query: string
  limit?: number
}

interface SuggestionsRequest {
  op: "suggestions"
  session: string
  limit?: number
}

interface FollowersRequest {
  op: "followers"
  session: string
  username: string
  limit?: number
}

interface FollowingRequest {
  op: "following"
  session: string
  username: string
  limit?: number
}

type Request =
  | RegisterRequest
  | LoginRequest
  | LogoutRequest
  | CreatePostRequest
  | FollowRequest
  | UnfollowRequest
  | LikeRequest
  | CommentRequest
  | FeedRequest
  | GetPostRequest
  | ProfileRequest
  | UpdateProfileRequest
  | SearchUsersRequest
  | SuggestionsRequest
  | FollowersRequest
  | FollowingRequest

interface OkResponse {
  status: "ok"
  message: string
}

interface LoginOkResponse {
  status: "login_ok"
  session: string
  expires_ms: number
}

interface PostResponse {
  status: "post"
  post: ApiPost
}

interface FeedResponse {
  status: "feed"
  posts: ApiPost[]
}

interface ProfileResponse {
  status: "profile"
  profile: {
    username: string
    display_name?: string | null
    bio?: string | null
    avatar_blob_id?: string | null
    banner_blob_id?: string | null
    followers: number
    following: number
    follower_list: string[]
    following_list: string[]
    posts: string[]
    created_at: number
  }
}

interface UsersResponse {
  status: "users"
  users: Array<{
    username: string
    display_name?: string | null
    bio?: string | null
    avatar_blob_id?: string | null
    banner_blob_id?: string | null
    followers: number
  }>
}

interface ErrorResponse {
  status: "error"
  message: string
}

type Response = OkResponse | LoginOkResponse | PostResponse | FeedResponse | ProfileResponse | UsersResponse | ErrorResponse
type AnyResponse = Partial<Response> & { status?: string; message?: string }

const RPC_ENDPOINT = process.env.NEXT_PUBLIC_ONVM_RPC_ENDPOINT || "http://localhost:8080"

export class NbookONVMClient {
  private rpcEndpoint: string
  private programId: string

  constructor(rpcEndpoint: string = RPC_ENDPOINT, programId: string = process.env.NEXT_PUBLIC_ONVM_PROGRAM_ID || "c4f18b0ae12975f4df2ff6739ae77f0d31cd2a4b11bb73fa96a6da2f3a0f3cd9") {
    this.rpcEndpoint = rpcEndpoint
    this.programId = programId
  }

  // Authentication operations
  async register(username: string, password: string): Promise<OkResponse> {
    const response = await this.executeProgram<OkResponse>({
      op: "register",
      username,
      password,
    })
    if (response.status === "error") {
      throw new Error(response.message || "register failed")
    }
    return response as OkResponse
  }

  async login(username: string, password: string): Promise<{ session: string; expiresMs: number }> {
    const response = await this.executeProgram<LoginOkResponse>({
      op: "login",
      username,
      password,
    })
    if (response.status === "error" || response.status !== "login_ok") {
      throw new Error((response as ErrorResponse).message || "login failed")
    }
    const loginResponse = response as LoginOkResponse
    return { session: loginResponse.session, expiresMs: loginResponse.expires_ms }
  }

  async deletePost(session: string, postId: string): Promise<void> {
    const response = await this.executeProgram<OkResponse>({
      op: "delete_post",
      session,
      post_id: postId,
    })
    if (response.status === "error") {
      throw new Error(response.message || "delete failed")
    }
  }

  async logout(session: string): Promise<void> {
    const response = await this.executeProgram<OkResponse>({
      op: "logout",
      session,
    })
    if (response.status === "error") {
      throw new Error(response.message || "logout failed")
    }
  }

  // Post operations
  async createPost(session: string, text: string, attachments: string[] = []): Promise<Post> {
    const response = await this.executeProgram<PostResponse>({
      op: "create_post",
      session,
      text,
      attachments,
    })
    if (response.status === "error") {
      throw new Error(response.message || "create post failed")
    }
    return this.normalizePost((response as PostResponse).post)
  }

  async getPost(postId: string): Promise<Post> {
    const response = await this.executeProgram<PostResponse>({
      op: "get_post",
      post_id: postId,
    })
    if (response.status === "error") {
      throw new Error(response.message || "get post failed")
    }
    return this.normalizePost((response as PostResponse).post)
  }

  async likePost(session: string, postId: string): Promise<Post> {
    const response = await this.executeProgram<PostResponse>({
      op: "like",
      session,
      post_id: postId,
    })
    if (response.status === "error") {
      throw new Error(response.message || "like failed")
    }
    return this.normalizePost((response as PostResponse).post)
  }

  async commentOnPost(session: string, postId: string, text: string): Promise<Post> {
    const response = await this.executeProgram<PostResponse>({
      op: "comment",
      session,
      post_id: postId,
      text,
    })
    if (response.status === "error") {
      throw new Error(response.message || "comment failed")
    }
    return this.normalizePost((response as PostResponse).post)
  }

  // Feed operations
  async getFeed(session: string, limit = 20, discover = false): Promise<Post[]> {
    const response = await this.executeProgram<FeedResponse>({
      op: "feed",
      session,
      limit,
      discover,
    })
    if (response.status === "error") {
      throw new Error(response.message || "feed failed")
    }
    return (response as FeedResponse).posts.map((p) => this.normalizePost(p))
  }

  // Social operations
  async follow(session: string, target: string): Promise<void> {
    const response = await this.executeProgram<OkResponse>({
      op: "follow",
      session,
      target,
    })
    if (response.status === "error") {
      throw new Error(response.message || "follow failed")
    }
  }

  async unfollow(session: string, target: string): Promise<void> {
    const response = await this.executeProgram<OkResponse>({
      op: "unfollow",
      session,
      target,
    })
    if (response.status === "error") {
      throw new Error(response.message || "unfollow failed")
    }
  }

  // Profile operations
  async getProfile(username: string): Promise<ProfileView> {
    const response = await this.executeProgram<ProfileResponse>({
      op: "profile",
      username,
    })
    if (response.status === "error") {
      throw new Error(response.message || "profile failed")
    }
    return this.normalizeProfile((response as ProfileResponse).profile)
  }

  async updateProfile(
    session: string,
    displayName?: string,
    bio?: string,
    avatarBlobId?: string,
    bannerBlobId?: string
  ): Promise<void> {
    const response = await this.executeProgram<OkResponse>({
      op: "update_profile",
      session,
      display_name: displayName,
      bio,
      avatar_blob_id: avatarBlobId,
      banner_blob_id: bannerBlobId,
    })
    if (response.status === "error") {
      throw new Error(response.message || "profile update failed")
    }
  }

  async searchUsers(session: string, query: string, limit = 10): Promise<UserSummary[]> {
    const response = await this.executeProgram<UsersResponse>({
      op: "search_users",
      session,
      query,
      limit,
    })
    if (response.status === "error") {
      throw new Error(response.message || "search failed")
    }
    return this.normalizeUsers((response as UsersResponse).users)
  }

  async getSuggestions(session: string, limit = 5): Promise<UserSummary[]> {
    const response = await this.executeProgram<UsersResponse>({
      op: "suggestions",
      session,
      limit,
    })
    if (response.status === "error") {
      throw new Error(response.message || "suggestions failed")
    }
    return this.normalizeUsers((response as UsersResponse).users)
  }

  async listFollowers(session: string, username: string, limit = 50): Promise<UserSummary[]> {
    const response = await this.executeProgram<UsersResponse>({
      op: "followers",
      session,
      username,
      limit,
    })
    if (response.status === "error") {
      throw new Error(response.message || "followers failed")
    }
    return this.normalizeUsers((response as UsersResponse).users)
  }

  async listFollowing(session: string, username: string, limit = 50): Promise<UserSummary[]> {
    const response = await this.executeProgram<UsersResponse>({
      op: "following",
      session,
      username,
      limit,
    })
    if (response.status === "error") {
      throw new Error(response.message || "following failed")
    }
    return this.normalizeUsers((response as UsersResponse).users)
  }

  // Generic execute program method
  private async executeProgram<T extends Response>(request: Request): Promise<T> {
    try {
      if (!this.programId) {
        throw new Error("NEXT_PUBLIC_ONVM_PROGRAM_ID is not set")
      }

      // Encode request as base64
      const inputBase64 = btoa(JSON.stringify(request))

      // Execute program via ONVM RPC
      const response = await fetch(`${this.rpcEndpoint}/execute`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          program_id: this.programId,
          input_base64: inputBase64,
        }),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`RPC error: ${response.status} - ${errorText}`)
      }

      const result = await response.json()

      // Decode the response
      const returnData = atob(result.return_base64)
      const parsedResponse = JSON.parse(returnData) as AnyResponse
      if (!parsedResponse.status) {
        throw new Error("invalid response from program")
      }
      return parsedResponse as T
    } catch (error) {
      console.error("[v0] ONVM Client Error:", error)
      throw error
    }
  }

  // Helper method to upload a WASM program
  async uploadWasm(wasmBuffer: ArrayBuffer): Promise<string> {
    try {
      // Upload WASM blob
      const blobResponse = await fetch(`${this.rpcEndpoint}/blobs`, {
        method: "POST",
        headers: {
          "Content-Type": "application/wasm",
        },
        body: wasmBuffer,
      })

      if (!blobResponse.ok) {
        const errorText = await blobResponse.text()
        throw new Error(`Blob upload error: ${blobResponse.status} - ${errorText}`)
      }

      const blobResult = await blobResponse.json()
      const blobId = blobResult.id

      // Convert ArrayBuffer to base64
      const bytes = new Uint8Array(wasmBuffer)
      let binary = ""
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i])
      }
      const wasmBase64 = btoa(binary)

      // Deploy program
      const deployResponse = await fetch(`${this.rpcEndpoint}/programs`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          wasm_base64: wasmBase64,
          entrypoint: "onvm_main",
          blob_refs: [],
        }),
      })

      if (!deployResponse.ok) {
        const errorText = await deployResponse.text()
        throw new Error(`Program deploy error: ${deployResponse.status} - ${errorText}`)
      }

      const deployResult = await deployResponse.json()
      return deployResult.id
    } catch (error) {
      console.error("[v0] WASM Upload Error:", error)
      throw error
    }
  }

  // Helper method to upload blobs (images, videos, etc.)
  async uploadBlob(buffer: ArrayBuffer): Promise<string> {
    try {
      const response = await fetch(`${this.rpcEndpoint}/blobs`, {
        method: "POST",
        headers: {
          "Content-Type": "application/octet-stream",
        },
        body: buffer,
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`Blob upload error: ${response.status} - ${errorText}`)
      }

      const result = await response.json()
      return result.id
    } catch (error) {
      console.error("[v0] Blob Upload Error:", error)
      throw error
    }
  }

  // Download a blob and return it as a Browser Blob
  async fetchBlob(id: string): Promise<{ blob: Blob; contentType: string }> {
    const response = await fetch(`${this.rpcEndpoint}/blobs/${id}`, {
      method: "GET",
    })
    if (!response.ok) {
      const txt = await response.text()
      throw new Error(`Blob fetch error: ${response.status} ${txt}`)
    }
    const headerCt = response.headers.get("content-type") || ""
    const buffer = await response.arrayBuffer()
    const sniffed = sniffContentType(buffer, headerCt)
    const blob = new Blob([buffer], { type: sniffed })
    return { blob, contentType: sniffed }
  }

  // Normalize API shapes (snake_case) to client shapes (camelCase)
  private normalizePost(api: ApiPost): Post {
    return {
      id: api.id,
      author: api.author,
      text: api.text,
      attachments: api.attachments,
      createdAt: api.created_at,
      likes: api.likes,
      comments: api.comments.map((c) => ({
        id: c.id,
        author: c.author,
        text: c.text,
        createdAt: c.created_at,
      })),
    }
  }

  private normalizeProfile(api: ProfileResponse["profile"]): ProfileView {
    return {
      username: api.username,
      displayName: api.display_name,
      bio: api.bio,
      avatarBlobId: api.avatar_blob_id,
      bannerBlobId: api.banner_blob_id,
      followers: api.followers,
      following: api.following,
      followerList: api.follower_list || [],
      followingList: api.following_list || [],
      posts: api.posts,
      createdAt: api.created_at,
    }
  }

  private normalizeUsers(users: UsersResponse["users"]): UserSummary[] {
    return users.map((u) => ({
      username: u.username,
      displayName: u.display_name,
      bio: u.bio,
      avatarBlobId: u.avatar_blob_id,
      bannerBlobId: u.banner_blob_id,
      followers: u.followers,
    }))
  }
}

// Singleton instance
let nbookClient: NbookONVMClient | null = null

export const getNbookClient = (): NbookONVMClient => {
  if (!nbookClient) {
    nbookClient = new NbookONVMClient()
  }
  return nbookClient
}

export default NbookONVMClient

// Naive content-type sniffing to improve media rendering when the RPC returns octet-stream
function sniffContentType(buffer: ArrayBuffer, headerCt: string): string {
  const defaultCt = headerCt && headerCt !== "application/octet-stream" ? headerCt : ""
  const bytes = new Uint8Array(buffer.slice(0, 12))

  // ID3 header or MPEG sync
  if (
    (bytes[0] === 0x49 && bytes[1] === 0x44 && bytes[2] === 0x33) || // "ID3"
    (bytes[0] === 0xff && (bytes[1] & 0xe0) === 0xe0)
  ) {
    return "audio/mpeg"
  }
  // OggS
  if (bytes[0] === 0x4f && bytes[1] === 0x67 && bytes[2] === 0x67 && bytes[3] === 0x53) {
    return "audio/ogg"
  }
  // RIFF....WAVE
  if (
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x41 &&
    bytes[10] === 0x56 &&
    bytes[11] === 0x45
  ) {
    return "audio/wav"
  }
  // MP4/QuickTime ftyp
  if (bytes[4] === 0x66 && bytes[5] === 0x74 && bytes[6] === 0x79 && bytes[7] === 0x70) {
    return "video/mp4"
  }
  return defaultCt || "application/octet-stream"
}
