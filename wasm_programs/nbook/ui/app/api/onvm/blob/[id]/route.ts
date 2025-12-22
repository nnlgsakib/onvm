import { NextRequest, NextResponse } from 'next/server'
import { OnvmClient } from 'onvm-sdk'

const RPC_ENDPOINT = process.env.ONVM_RPC_ENDPOINT
const PROJECT_ID = process.env.ONVM_PROJECT_ID
const PROJECT_SECRET = process.env.PROJECT_SECRET

let clientInstance: OnvmClient | null = null

function getClient(): OnvmClient {
  if (!clientInstance) {
    clientInstance = new OnvmClient({
      rpcUrl: RPC_ENDPOINT || '',
      projectId: PROJECT_ID,
      projectSecret: PROJECT_SECRET,
      timeout: 30000,
    })
  }
  return clientInstance
}

export async function GET(
  _req: NextRequest,
  context: { params: Promise<{ id: string }> }
) {
  const { id: blobId } = await context.params
  if (!blobId) {
    return NextResponse.json({ error: 'missing blob id' }, { status: 400 })
  }

  try {
    const client = getClient()
    const buffer = await client.downloadBlob(blobId)
    const contentType = sniffContentType(buffer)
    return new NextResponse(buffer, {
      status: 200,
      headers: {
        'Content-Type': contentType,
        'Cache-Control': 'public, max-age=60',
      },
    })
  } catch (error) {
    console.error('ONVM blob proxy error:', error)
    return NextResponse.json(
      { error: error instanceof Error ? error.message : 'Failed to fetch blob' },
      { status: 500 }
    )
  }
}

// Minimal magic-number sniffing to set a friendly content-type header
function sniffContentType(buffer: Buffer): string {
  if (buffer.length >= 8) {
    if (
      buffer[0] === 0x89 &&
      buffer[1] === 0x50 &&
      buffer[2] === 0x4e &&
      buffer[3] === 0x47
    ) {
      return 'image/png'
    }
    if (
      buffer[0] === 0xff &&
      buffer[1] === 0xd8 &&
      buffer[2] === 0xff
    ) {
      return 'image/jpeg'
    }
    if (
      buffer[0] === 0x47 &&
      buffer[1] === 0x49 &&
      buffer[2] === 0x46 &&
      buffer[3] === 0x38
    ) {
      return 'image/gif'
    }
    if (
      buffer[0] === 0x52 &&
      buffer[1] === 0x49 &&
      buffer[2] === 0x46 &&
      buffer[3] === 0x46 &&
      buffer.length >= 12 &&
      buffer[8] === 0x57 &&
      buffer[9] === 0x45 &&
      buffer[10] === 0x42 &&
      buffer[11] === 0x50
    ) {
      return 'image/webp'
    }
  }
  const prefix = buffer.slice(0, 5).toString('utf8')
  if (prefix.startsWith('<?xml') || prefix.startsWith('<svg')) {
    return 'image/svg+xml'
  }
  return 'application/octet-stream'
}
