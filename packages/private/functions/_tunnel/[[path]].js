// Cloudflare Workers 프록시 - bore.pub 터널을 위한 HTTPS 래퍼
// GET/PUT/DELETE /api/tunnel?server=bore.pub:48681&path=/api/files

export async function onRequest(context) {
  const { request } = context;
  const url = new URL(request.url);
  
  // 쿼리 파라미터에서 대상 서버와 경로 추출
  const server = url.searchParams.get('server');
  const targetPath = url.searchParams.get('path') || '/';
  
  console.log('[Tunnel] 요청:', { server, targetPath, method: request.method });
  
  if (!server) {
    console.log('[Tunnel] 에러: server 파라미터 없음');
    return Response.json({ error: 'server parameter required' }, { status: 400 });
  }
  
  // 대상 URL 생성 (trycloudflare.com은 https, 나머지는 http)
  const protocol = server.includes('trycloudflare.com') ? 'https' : 'http';
  const targetUrl = `${protocol}://${server}${targetPath}`;
  console.log('[Tunnel] 대상 URL:', targetUrl);
  
  // 원본 요청의 헤더 복사 (Host는 대상 서버로 설정)
  const headers = new Headers();
  for (const [key, value] of request.headers.entries()) {
    if (key.toLowerCase() !== 'host') {
      headers.set(key, value);
    }
  }
  // Host 헤더를 대상 서버로 설정 (Cloudflare 터널 필수)
  headers.set('Host', server);
  
  try {
    console.log('[Tunnel] fetch 시작...');
    // 대상 서버로 요청 전달
    const response = await fetch(targetUrl, {
      method: request.method,
      headers,
      body: request.method !== 'GET' && request.method !== 'HEAD' 
        ? await request.text() 
        : undefined,
    });
    
    console.log('[Tunnel] 응답:', response.status);
    
    // CORS 헤더 추가
    const responseHeaders = new Headers(response.headers);
    responseHeaders.set('Access-Control-Allow-Origin', '*');
    responseHeaders.set('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
    responseHeaders.set('Access-Control-Allow-Headers', 'Content-Type, Authorization');
    
    return new Response(response.body, {
      status: response.status,
      headers: responseHeaders,
    });
  } catch (err) {
    console.error('[Tunnel] 연결 실패:', err.message);
    return Response.json({ 
      error: 'Tunnel connection failed', 
      details: err.message,
      targetUrl 
    }, { status: 502 });
  }
}

// OPTIONS 요청 처리 (CORS preflight)
export async function onRequestOptions() {
  return new Response(null, {
    headers: {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    },
  });
}
