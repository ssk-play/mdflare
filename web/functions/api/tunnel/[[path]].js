// Cloudflare Workers 프록시 - bore.pub 터널을 위한 HTTPS 래퍼
// GET/PUT/DELETE /api/tunnel?server=bore.pub:48681&path=/api/files

export async function onRequest(context) {
  const { request } = context;
  const url = new URL(request.url);
  
  // 쿼리 파라미터에서 대상 서버와 경로 추출
  const server = url.searchParams.get('server');
  const targetPath = url.searchParams.get('path') || '/';
  
  if (!server) {
    return Response.json({ error: 'server parameter required' }, { status: 400 });
  }
  
  // 대상 URL 생성
  const targetUrl = `http://${server}${targetPath}`;
  
  // 원본 요청의 헤더 복사 (Host 제외)
  const headers = new Headers();
  for (const [key, value] of request.headers.entries()) {
    if (key.toLowerCase() !== 'host') {
      headers.set(key, value);
    }
  }
  
  try {
    // 대상 서버로 요청 전달
    const response = await fetch(targetUrl, {
      method: request.method,
      headers,
      body: request.method !== 'GET' && request.method !== 'HEAD' 
        ? await request.text() 
        : undefined,
    });
    
    // CORS 헤더 추가
    const responseHeaders = new Headers(response.headers);
    responseHeaders.set('Access-Control-Allow-Origin', '*');
    responseHeaders.set('Access-Control-Allow-Methods', 'GET, PUT, DELETE, OPTIONS');
    responseHeaders.set('Access-Control-Allow-Headers', 'Content-Type, Authorization');
    
    return new Response(response.body, {
      status: response.status,
      headers: responseHeaders,
    });
  } catch (err) {
    return Response.json({ error: 'Tunnel connection failed', details: err.message }, { status: 502 });
  }
}

// OPTIONS 요청 처리 (CORS preflight)
export async function onRequestOptions() {
  return new Response(null, {
    headers: {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, PUT, DELETE, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    },
  });
}
