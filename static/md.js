// Minimal markdown renderer for item bodies and documents, board.py's, kept
// as it was. Self-contained on purpose: the board runs offline, so no CDN.
// Covers the subset the bodies use (headers, bold, inline/fenced code, lists,
// tables, blockquotes, hr, links, [[wikilinks]], :emoji:); anything
// unrecognised passes through.
const esc=s=>String(s).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const MDEMOJI={warning:'⚠️',memo:'📝',heavy_plus_sign:'➕',heavy_check_mark:'✔️',x:'❌',rocket:'🚀',bulb:'💡',wrench:'🔧',bug:'🐛',zap:'⚡',fire:'🔥',point_right:'👉'};
function mdInline(s){
  const codes=[];
  s=esc(s).replace(/`([^`]+)`/g,(_,c)=>{codes.push(c);return '\u0000'+(codes.length-1)+'\u0000';});
  s=s.replace(/\[\[([^\]]+)\]\]/g,'<span class=wl>$1</span>')
     .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g,(m,t,u)=>/^https?:\/\//.test(u)?'<a href="'+u+'" target=_blank rel=noopener>'+t+'</a>':'<span class=wl>'+t+'</span>')
     .replace(/\*\*([^*]+)\*\*/g,'<strong>$1</strong>')
     .replace(/(^|[^*\w])\*([^*\n]+)\*(?![*\w])/g,'$1<em>$2</em>')
     .replace(/:([a-z_+0-9]+):/g,(m,n)=>MDEMOJI[n]||m);
  return s.replace(/\u0000(\d+)\u0000/g,(_,i)=>'<code>'+codes[+i]+'</code>');
}
function md(src){
  const L=(src||'').replace(/\r/g,'').split('\n'); const out=[]; let i=0;
  const special=s=>/^(#{1,6}\s|>|\||```|\s*[-*]\s|\s*\d+\.\s|---\s*$|\*\*\*\s*$)/.test(s);
  while(i<L.length){
    const ln=L[i];
    if(/^```/.test(ln)){const b=[];i++;while(i<L.length&&!/^```/.test(L[i]))b.push(L[i++]);i++;out.push('<pre><code>'+esc(b.join('\n'))+'</code></pre>');continue;}
    let m;
    if(m=ln.match(/^(#{1,6})\s+(.*)/)){const n=m[1].length;out.push('<h'+n+'>'+mdInline(m[2])+'</h'+n+'>');i++;continue;}
    if(/^(---|\*\*\*|___)\s*$/.test(ln)){out.push('<hr>');i++;continue;}
    if(/^>\s?/.test(ln)){const b=[];while(i<L.length&&/^>\s?/.test(L[i]))b.push(L[i++].replace(/^>\s?/,''));out.push('<blockquote>'+md(b.join('\n'))+'</blockquote>');continue;}
    if(/^\s*\|/.test(ln)&&i+1<L.length&&/^\s*\|?[\s:|-]+\|?\s*$/.test(L[i+1])&&L[i+1].includes('-')){
      const cells=r=>r.replace(/^\s*\|/,'').replace(/\|\s*$/,'').split('|').map(c=>mdInline(c.trim()));
      const head=cells(ln);i+=2;const rows=[];
      while(i<L.length&&/^\s*\|/.test(L[i]))rows.push(cells(L[i++]));
      out.push('<table><thead><tr>'+head.map(c=>'<th>'+c+'</th>').join('')+'</tr></thead><tbody>'+rows.map(r=>'<tr>'+r.map(c=>'<td>'+c+'</td>').join('')+'</tr>').join('')+'</tbody></table>');continue;
    }
    if(/^\s*[-*]\s+/.test(ln)){const b=[];while(i<L.length&&/^\s*[-*]\s+/.test(L[i]))b.push('<li>'+mdInline(L[i++].replace(/^\s*[-*]\s+/,''))+'</li>');out.push('<ul>'+b.join('')+'</ul>');continue;}
    if(/^\s*\d+\.\s+/.test(ln)){const b=[];while(i<L.length&&/^\s*\d+\.\s+/.test(L[i]))b.push('<li>'+mdInline(L[i++].replace(/^\s*\d+\.\s+/,''))+'</li>');out.push('<ol>'+b.join('')+'</ol>');continue;}
    if(/^\s*$/.test(ln)){i++;continue;}
    const b=[];while(i<L.length&&!/^\s*$/.test(L[i])&&!special(L[i]))b.push(L[i++]);
    out.push('<p>'+mdInline(b.join(' '))+'</p>');
  }
  return out.join('');
}

// Shared by the board and item pages.
const KINDS=['URGENT','BUG','DEBT','KILLER','TODO'];
const STATUSES=['open','doing','done','wontfix'];
const kindClass=k=>'kind-'+String(k||'').toLowerCase();
const isClosed=i=>i.status==='done'||i.status==='wontfix';
async function api(path,body){
  const r=await fetch(API+path,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await r.json().catch(()=>({}));
  if(!r.ok)throw new Error(data.error||('HTTP '+r.status));
  return data;
}
Object.assign(window,{esc,md,mdInline,KINDS,STATUSES,kindClass,isClosed,api});
