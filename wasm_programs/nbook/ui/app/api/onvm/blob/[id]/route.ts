import { NextRequest, NextResponse } from 'next/server'
import { OnvmClient } from 'onvm-sdk'

const RPC_ENDPOINT = process.env.ONVM_RPC_ENDPOINT
const PROGRAM_ID = process.env.ONVM_PROGRAM_ID
const PROJECT_ID = process.env.ONVM_PROJECT_ID
const PROJECT_SECRET = process.env.PROJECT_SECRET

let clientInstance: OnvmClient | null = null

function getClient(): OnvmClient {
  if (!clientInstance) {
    clientInstance = new OnvmClient({
      rpcUrl: RPC_ENDPOINT || '',
      programId: PROGRAM_ID,
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
    return new NextResponse(buffer, {
      status: 200,
      headers: {
        'Content-Type': 'application/octet-stream',
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
