pub fn generate_share_page(
    id: &str,
    mime_type: &str,
    size: &str,
    preview_html: &str,
    cdn_url: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ONVM Share - {}</title>
    <meta property="og:title" content="Shared Content on ONVM">
    <meta property="og:description" content="View shared content on ONVM decentralized network">
    <meta property="og:type" content="website">
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }}

        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: rgba(255, 255, 255, 0.95);
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
            padding: 40px;
        }}

        h1 {{
            color: #333;
            margin-bottom: 10px;
            font-size: 2em;
        }}

        .subtitle {{
            color: #666;
            margin-bottom: 30px;
            font-size: 1.1em;
        }}

        .info-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 10px;
        }}

        .info-item {{
            display: flex;
            flex-direction: column;
            gap: 5px;
        }}

        .info-label {{
            font-weight: 600;
            color: #555;
            font-size: 0.9em;
        }}

        .info-value {{
            color: #333;
            font-family: monospace;
            word-break: break-all;
        }}

        .preview {{
            margin: 30px 0;
            text-align: center;
        }}

        .actions {{
            display: flex;
            gap: 15px;
            flex-wrap: wrap;
            margin-top: 30px;
        }}

        .btn {{
            padding: 12px 24px;
            border: none;
            border-radius: 10px;
            font-size: 16px;
            font-weight: 600;
            cursor: pointer;
            transition: transform 0.2s, box-shadow 0.2s;
            text-decoration: none;
            display: inline-block;
        }}

        .btn-primary {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
        }}

        .btn-secondary {{
            background: #f8f9fa;
            color: #333;
            border: 2px solid #ddd;
        }}

        .btn:hover {{
            transform: translateY(-2px);
            box-shadow: 0 5px 15px rgba(102, 126, 234, 0.4);
        }}

        .share-links {{
            margin-top: 30px;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 10px;
        }}

        .share-links h3 {{
            margin-bottom: 15px;
            color: #333;
        }}

        .share-link {{
            display: flex;
            gap: 10px;
            margin: 10px 0;
        }}

        .share-link input {{
            flex: 1;
            padding: 10px;
            border: 2px solid #ddd;
            border-radius: 8px;
            font-family: monospace;
            font-size: 14px;
        }}

        .copy-btn {{
            padding: 10px 20px;
            background: #667eea;
            color: white;
            border: none;
            border-radius: 8px;
            cursor: pointer;
            font-weight: 600;
        }}

        .copy-btn:hover {{
            background: #5568d3;
        }}

        .toast {{
            position: fixed;
            top: 20px;
            right: 20px;
            padding: 15px 25px;
            background: #10b981;
            color: white;
            border-radius: 10px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.2);
            opacity: 0;
            transition: opacity 0.3s;
            z-index: 1000;
        }}

        .toast.show {{
            opacity: 1;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>🌐 ONVM Shared Content</h1>
        <p class="subtitle">Decentralized Content Delivery Network</p>

        <div class="info-grid">
            <div class="info-item">
                <span class="info-label">Content ID</span>
                <span class="info-value">{}</span>
            </div>
            <div class="info-item">
                <span class="info-label">Type</span>
                <span class="info-value">{}</span>
            </div>
            <div class="info-item">
                <span class="info-label">Size</span>
                <span class="info-value">{}</span>
            </div>
        </div>

        <div class="preview">
            {}
        </div>

        <div class="actions">
            <a href="{}" download class="btn btn-primary">Download</a>
            <button onclick="copyDirectLink()" class="btn btn-secondary">Copy Direct Link</button>
            <button onclick="copyEmbedCode()" class="btn btn-secondary">Copy Embed Code</button>
        </div>

        <div class="share-links">
            <h3>Share Links</h3>
            <div class="share-link">
                <input type="text" id="directLink" readonly value="{{DIRECT_LINK}}">
                <button class="copy-btn" onclick="copyToClipboard('directLink')">Copy</button>
            </div>
            <div class="share-link">
                <input type="text" id="embedCode" readonly value="{{EMBED_CODE}}">
                <button class="copy-btn" onclick="copyToClipboard('embedCode')">Copy</button>
            </div>
        </div>
    </div>

    <div class="toast" id="toast"></div>

    <script>
        const directLink = window.location.origin + '{}';
        const embedCode = '<img src="' + directLink + '">';
        
        document.getElementById('directLink').value = directLink;
        document.getElementById('embedCode').value = embedCode;

        function copyToClipboard(elementId) {{
            const input = document.getElementById(elementId);
            input.select();
            document.execCommand('copy');
            showToast('Copied to clipboard!');
        }}

        function copyDirectLink() {{
            copyToClipboard('directLink');
        }}

        function copyEmbedCode() {{
            copyToClipboard('embedCode');
        }}

        function showToast(message) {{
            const toast = document.getElementById('toast');
            toast.textContent = message;
            toast.className = 'toast show';
            setTimeout(() => {{
                toast.className = 'toast';
            }}, 3000);
        }}
    </script>
</body>
</html>"#,
        &id[..16],
        &id[..16],
        mime_type,
        size,
        preview_html,
        cdn_url,
        cdn_url
    )
}

pub const EXPLORER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ONVM Blob Gateway</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }

        .container {
            background: rgba(255, 255, 255, 0.95);
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
            padding: 40px;
            max-width: 800px;
            width: 100%;
        }

        h1 {
            color: #333;
            margin-bottom: 10px;
            font-size: 2.5em;
            text-align: center;
        }

        .subtitle {
            text-align: center;
            color: #666;
            margin-bottom: 30px;
            font-size: 1.1em;
        }

        .search-section {
            margin-bottom: 30px;
        }

        .search-box {
            display: flex;
            gap: 10px;
            margin-bottom: 20px;
        }

        input[type="text"] {
            flex: 1;
            padding: 15px;
            border: 2px solid #ddd;
            border-radius: 10px;
            font-size: 16px;
            transition: border-color 0.3s;
        }

        input[type="text"]:focus {
            outline: none;
            border-color: #667eea;
        }

        button {
            padding: 15px 30px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            border: none;
            border-radius: 10px;
            font-size: 16px;
            cursor: pointer;
            transition: transform 0.2s, box-shadow 0.2s;
            font-weight: 600;
        }

        button:hover {
            transform: translateY(-2px);
            box-shadow: 0 5px 15px rgba(102, 126, 234, 0.4);
        }

        button:active {
            transform: translateY(0);
        }

        .info-section {
            background: #f8f9fa;
            border-radius: 10px;
            padding: 20px;
            margin-bottom: 20px;
            display: none;
        }

        .info-section.show {
            display: block;
        }

        .info-item {
            margin-bottom: 10px;
            display: flex;
            justify-content: space-between;
            padding: 10px;
            background: white;
            border-radius: 5px;
        }

        .info-label {
            font-weight: 600;
            color: #555;
        }

        .info-value {
            color: #333;
            font-family: monospace;
        }

        .preview-section {
            margin-top: 20px;
            display: none;
            border-radius: 10px;
            overflow: hidden;
            background: #000;
        }

        .preview-section.show {
            display: block;
        }

        .preview-section img,
        .preview-section video,
        .preview-section audio {
            width: 100%;
            max-height: 500px;
            object-fit: contain;
        }

        .error-message {
            background: #fee;
            color: #c33;
            padding: 15px;
            border-radius: 10px;
            margin-top: 20px;
            display: none;
        }

        .error-message.show {
            display: block;
        }

        .success-message {
            background: #efe;
            color: #3a3;
            padding: 15px;
            border-radius: 10px;
            margin-top: 20px;
            display: none;
        }

        .success-message.show {
            display: block;
        }

        .loading {
            text-align: center;
            padding: 20px;
            display: none;
        }

        .loading.show {
            display: block;
        }

        .spinner {
            border: 4px solid #f3f3f3;
            border-top: 4px solid #667eea;
            border-radius: 50%;
            width: 40px;
            height: 40px;
            animation: spin 1s linear infinite;
            margin: 0 auto;
        }

        @keyframes spin {
            0% { transform: rotate(0deg); }
            100% { transform: rotate(360deg); }
        }

        .actions {
            display: flex;
            gap: 10px;
            margin-top: 20px;
        }

        .actions button {
            flex: 1;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🌐 ONVM Blob Gateway</h1>
        <p class="subtitle">Decentralized Content Delivery Network</p>

        <div class="search-section">
            <div class="search-box">
                <input type="text" id="blobId" placeholder="Enter Blob ID (hex)">
                <button onclick="fetchBlobInfo()">Fetch Info</button>
            </div>
        </div>

        <div class="loading" id="loading">
            <div class="spinner"></div>
            <p>Loading...</p>
        </div>

        <div class="error-message" id="error"></div>
        <div class="success-message" id="success"></div>

        <div class="info-section" id="infoSection">
            <h3>Blob Information</h3>
            <div class="info-item">
                <span class="info-label">ID:</span>
                <span class="info-value" id="infoId">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">Size:</span>
                <span class="info-value" id="infoSize">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">Chunks:</span>
                <span class="info-value" id="infoChunks">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">Subchunks:</span>
                <span class="info-value" id="infoSubchunks">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">MIME Type:</span>
                <span class="info-value" id="infoMime">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">Detected Type:</span>
                <span class="info-value" id="infoDetectedMime">-</span>
            </div>
            <div class="info-item">
                <span class="info-label">Local:</span>
                <span class="info-value" id="infoLocal">-</span>
            </div>

            <div class="actions">
                <button onclick="viewBlob()">View</button>
                <button onclick="downloadBlob()">Download</button>
                <button onclick="shareBlob()">Share</button>
                <button onclick="copyDirectLink()">Copy CDN Link</button>
            </div>
        </div>

        <div class="preview-section" id="previewSection"></div>
    </div>

    <script>
        function showLoading(show) {
            document.getElementById('loading').className = show ? 'loading show' : 'loading';
        }

        function showError(message) {
            const el = document.getElementById('error');
            el.textContent = message;
            el.className = 'error-message show';
            setTimeout(() => el.className = 'error-message', 5000);
        }

        function showSuccess(message) {
            const el = document.getElementById('success');
            el.textContent = message;
            el.className = 'success-message show';
            setTimeout(() => el.className = 'success-message', 3000);
        }

        function formatSize(bytes) {
            if (bytes < 1024) return bytes + ' B';
            if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(2) + ' KB';
            if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(2) + ' MB';
            return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
        }

        async function fetchBlobInfo() {
            const id = document.getElementById('blobId').value.trim();
            if (!id) {
                showError('Please enter a Blob ID');
                return;
            }

            showLoading(true);
            document.getElementById('infoSection').className = 'info-section';
            document.getElementById('previewSection').className = 'preview-section';

            try {
                const response = await fetch(`/api/blob/${id}/info`);
                if (!response.ok) {
                    throw new Error(await response.text());
                }

                const info = await response.json();

                document.getElementById('infoId').textContent = info.id.substring(0, 16) + '...';
                document.getElementById('infoSize').textContent = formatSize(info.size);
                document.getElementById('infoChunks').textContent = info.chunk_count;
                document.getElementById('infoSubchunks').textContent = info.subchunk_count;
                document.getElementById('infoMime').textContent = info.mime_type || 'Unknown';
                document.getElementById('infoDetectedMime').textContent = info.detected_mime_type || 'Not detected yet';
                document.getElementById('infoLocal').textContent = info.available_locally ? 'Yes' : 'No';

                document.getElementById('infoSection').className = 'info-section show';
                showSuccess('Blob info loaded successfully');
            } catch (error) {
                showError('Error: ' + error.message);
            } finally {
                showLoading(false);
            }
        }

        async function viewBlob() {
            const id = document.getElementById('blobId').value.trim();
            if (!id) return;

            showLoading(true);
            const previewEl = document.getElementById('previewSection');
            previewEl.innerHTML = '';

            try {
                const response = await fetch(`/api/blob/${id}`);
                if (!response.ok) {
                    throw new Error(await response.text());
                }

                const contentType = response.headers.get('content-type');
                const blob = await response.blob();
                const url = URL.createObjectURL(blob);

                if (contentType.startsWith('image/')) {
                    const img = document.createElement('img');
                    img.src = url;
                    previewEl.appendChild(img);
                } else if (contentType.startsWith('video/')) {
                    const video = document.createElement('video');
                    video.src = url;
                    video.controls = true;
                    previewEl.appendChild(video);
                } else if (contentType.startsWith('audio/')) {
                    const audio = document.createElement('audio');
                    audio.src = url;
                    audio.controls = true;
                    previewEl.appendChild(audio);
                } else {
                    showError('Preview not available for this content type');
                    return;
                }

                previewEl.className = 'preview-section show';
                showSuccess('Blob loaded successfully');
            } catch (error) {
                showError('Error: ' + error.message);
            } finally {
                showLoading(false);
            }
        }

        async function downloadBlob() {
            const id = document.getElementById('blobId').value.trim();
            if (!id) return;

            showLoading(true);

            try {
                const response = await fetch(`/api/blob/${id}?download=true`);
                if (!response.ok) {
                    throw new Error(await response.text());
                }

                const blob = await response.blob();
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;

                const disposition = response.headers.get('content-disposition');
                let filename = 'download';
                if (disposition) {
                    const match = disposition.match(/filename="([^"]+)"/);
                    if (match) filename = match[1];
                }
                a.download = filename;

                document.body.appendChild(a);
                a.click();
                document.body.removeChild(a);
                URL.revokeObjectURL(url);

                showSuccess('Download started');
            } catch (error) {
                showError('Error: ' + error.message);
            } finally {
                showLoading(false);
            }
        }

        function shareBlob() {
            const id = document.getElementById('blobId').value.trim();
            if (!id) {
                showError('Please fetch blob info first');
                return;
            }
            const shareUrl = window.location.origin + '/share/' + id;
            window.open(shareUrl, '_blank');
        }

        function copyDirectLink() {
            const id = document.getElementById('blobId').value.trim();
            if (!id) {
                showError('Please fetch blob info first');
                return;
            }
            const cdnUrl = window.location.origin + '/cdn/' + id;
            navigator.clipboard.writeText(cdnUrl).then(() => {
                showSuccess('CDN link copied to clipboard!');
            }).catch(() => {
                showError('Failed to copy link');
            });
        }
    </script>
</body>
</html>
"#;
