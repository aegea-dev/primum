// One item's page: its text and controls on the left, its documents as tabs
// on the right, with the item's history as the last tab. board.py's
// expanded row, as a page of its own; `#<path>` selects a document's tab,
// `#history` the history.
const $=s=>document.querySelector(s);
// EDITING: Edit was clicked; the item's fields are fields until save or cancel.
// DOC: the documents' tab shown, a path or #history; DOCEDIT: its text is
// being edited.
let IT=null, HISTORY=[], EDITING=false, DOC=null, DOCEDIT=false;
const docCache={};

async function load(){
  const items=await api('/items');
  IT=items.find(i=>i.id===ITEM_ID);
  if(!IT){location.href='../';return;}
  knowItems(items,'');
  HISTORY=await api('/history?id='+encodeURIComponent(ITEM_ID)).catch(()=>[]);
  draw();
}

function draw(){
  const closed=isClosed(IT);
  $('#item-id').className='id '+kindClass(IT.kind);
  $('#item-id').textContent=label(IT.id);
  $('#item-id').title=IT.kind;
  $('#item-title').textContent=IT.title;
  document.title=label(IT.id)+' '+IT.title+' · primum';
  drawFields();
  $('#item-meta').innerHTML=`<span class=docpath>${esc(IT.file)}</span>`;
  $('#view').innerHTML=IT.body.trim()?md(IT.body):'<p class=empty>(no text yet)</p>';
  $('#kind').innerHTML=KINDS.map(k=>`<option ${k===IT.kind?'selected':''}>${k}</option>`).join('');
  drawConflicts();
  drawDocs();
}

// The item's fields: values at rest, fields while editing. A closed item
// has no edit: its status changes on the board.
function drawFields(){
  const value=f=>`<b class=val data-v=${f}>${esc(IT[f])}</b>`;
  const score=f=>`<label>${f} ${EDITING
    ?`<input class=num type=number min=1 max=10 value="${IT[f]}" data-f=${f} aria-label=${f}>`
    :value(f)}</label>`;
  $('#controls').innerHTML=
    `<label>status ${EDITING
      ?`<select data-f=status aria-label=status>${STATUSES.map(s=>`<option ${s===IT.status?'selected':''}>${s}</option>`).join('')}</select>`
      :value('status')}</label>`
    +['impact','urgency','risk','effort'].map(score).join('')
    +`<span>score <span class=score>${IT.score}</span></span>`
    +(EDITING
      ?`<label>needs <input type=text class=needs data-f=needs value="${esc(IT.needs.map(label).join(', '))}" placeholder="#1, #2" aria-label=needs></label>`
      // each a link to its item, as #12 in the text; what is not done yet warns
      :IT.needs.length?`<span>needs ${IT.blockers.length?'<span class=warn>'+mdInline(IT.blockers.map(label).join(', '))+'</span>':mdInline(IT.needs.map(label).join(', '))}</span>`:'')
    +(IT.finished?`<span>finished ${esc(IT.finished)}</span>`:'');
  // while editing, the score follows the scores before they are saved
  document.querySelectorAll('#controls input.num').forEach(el=>el.oninput=()=>{
    const v=f=>parseInt($(`#controls input[data-f=${f}]`).value,10);
    const s=scoreOf(v('impact'),v('urgency'),v('risk'),v('effort'));
    $('#controls .score').textContent=Number.isNaN(s)?'?':s;
  });
  $('#item-title').classList.toggle('hide',EDITING);
  let title=$('#title-field');
  if(EDITING&&!title){
    title=document.createElement('input');
    Object.assign(title,{type:'text',id:'title-field',value:IT.title});
    title.setAttribute('aria-label','title');
    $('#item-title').after(title);
  }
  if(!EDITING&&title)title.remove();
  $('#kind').classList.toggle('hide',!EDITING);
  $('#view').classList.toggle('hide',EDITING);
  $('#editor').classList.toggle('hide',!EDITING);
  $('#save').classList.toggle('hide',!EDITING);
  $('#cancel').classList.toggle('hide',!EDITING);
  $('#edit').classList.toggle('hide',EDITING||isClosed(IT));
  alignBars();
}

// Side by side, the two heads' button rows share a line: the shorter head
// gets the difference above its row.
function alignBars(){
  const rows=[$('section.left .sidehead .bar'),$('#docpane .sidehead .bar')];
  rows.forEach(r=>{r.style.marginTop='';});
  if(getComputedStyle($('#divider')).display==='none'||rows.some(r=>!r.offsetParent))return;
  const [left,right]=rows.map(r=>r.getBoundingClientRect().top);
  const higher=left<right?rows[0]:rows[1];
  higher.style.marginTop=parseFloat(getComputedStyle(higher).marginTop)+Math.abs(left-right)+'px';
}
window.addEventListener('resize',()=>alignBars());

// What a sync could not decide: both sides of each field, the text and
// each document, with "keep" buttons.
function drawConflicts(){
  const c=IT.conflict, box=$('#conflicts');
  box.classList.toggle('hide',!c);
  if(!c)return;
  const who=i=>`${esc(c.sides[i].author)}${c.sides[i].date?', '+esc(c.sides[i].date.slice(0,16).replace('T',' ')):''}`;
  const row=(what,label,a,b,both)=>`<div class=conflict data-what="${esc(what)}"><b>${esc(label)}</b>
    <button type=button data-keep=1>keep ${a} <small>${who(0)}</small></button>
    <button type=button data-keep=2>keep ${b} <small>${who(1)}</small></button>
    ${both?'<button type=button data-keep=both>keep both</button>':''}</div>`;
  box.innerHTML='<h3>Changed on two sides</h3>'
    +(c.fields||[]).map(f=>row(f.field,f.field,esc(f.values[0]),esc(f.values[1]),false)).join('')
    +(c.body?`<div class=conflict data-what=body><b>text</b><div class=bodies><div class=md>${md(c.body.values[0])}</div><div class=md>${md(c.body.values[1])}</div></div>
       <button type=button data-keep=1>keep the first <small>${who(0)}</small></button><button type=button data-keep=2>keep the second <small>${who(1)}</small></button></div>`:'')
    +(c.documents||[]).map(d=>row(d.path,d.path,'this one',esc(d.other.split('/').pop()),true)).join('');
  box.querySelectorAll('button[data-keep]').forEach(b=>b.onclick=async()=>{
    const what=b.closest('.conflict').dataset.what, k=b.dataset.keep;
    try{await api('/resolve',{id:IT.id,field:what,keep:k==='both'?'both':+k});}catch(e){alert(e.message);return;}
    await load();
  });
}

function drawDocs(){
  const docs=IT.docs, any=docs.length||HISTORY.length;
  $('#split').classList.toggle('single',!any);
  $('#docpane').classList.toggle('hide',!any);
  if(!any)return;
  const label=d=>d.kind+(d.path.startsWith('items/'+IT.stem+'.')?'':': '+d.name.replace(/[.]md$/,''));
  $('#doctabs').innerHTML=docs.map(d=>`<button type=button data-doc="${esc(d.path)}" title="${esc(d.name)}">${esc(label(d))}</button>`).join('')
    +(HISTORY.length?`<button type=button data-doc="#history" title="every change to this item">history (${HISTORY.length})</button>`:'');
  document.querySelectorAll('#doctabs button').forEach(b=>b.onclick=()=>{
    if(b.dataset.doc===DOC)return;
    if(DOCEDIT&&$('#doceditor').value!==docCache[DOC]&&!confirm('Drop your changes to '+DOC+'?'))return;
    docEditing(false);
    history.replaceState(null,'','#'+(b.dataset.doc==='#history'?'history':encodeURIComponent(b.dataset.doc)));
    show(b.dataset.doc);
  });
  if(DOCEDIT)return show(DOC);
  const want=decodeURIComponent(location.hash.slice(1));
  if(want==='history'&&HISTORY.length)return show('#history');
  show(docs.some(d=>d.path===want)?want:(docs.length?docs[0].path:'#history'));
}

function show(tab){
  DOC=tab;
  document.querySelectorAll('#doctabs button').forEach(b=>b.classList.toggle('on',b.dataset.doc===tab));
  drawDocBar();
  return tab==='#history'?showHistory():showDoc(tab);
}

// Each change: when, who, what; opening one shows its diff.
function showHistory(){
  $('#docpath').textContent=IT.file;
  $('#doc').innerHTML='<ol class=history>'+HISTORY.map(e=>
    `<li><details data-commit="${e.commit}" data-path="${esc(e.path||IT.file)}"><summary><span class=when>${esc(e.date.slice(0,16).replace('T',' '))}</span> <span class=who>${esc(e.author)}</span> ${esc(e.message)}</summary><pre class=diff></pre></details></li>`).join('')+'</ol>';
  document.querySelectorAll('#doc details').forEach(d=>d.ontoggle=async()=>{
    const pre=d.querySelector('pre');
    if(!d.open||pre.dataset.loaded)return;
    const r=await fetch(API+'/history/diff?commit='+d.dataset.commit+'&path='+encodeURIComponent(d.dataset.path));
    pre.innerHTML=(await r.text()).split('\n').map(l=>`<span class="${l[0]==='+'?'add':l[0]==='-'?'del':''}">${esc(l)}</span>`).join('\n');
    pre.dataset.loaded=1;
  });
}

async function showDoc(path){
  $('#docpath').textContent=path;
  const pane=$('#doc');
  if(!(path in docCache)){
    pane.innerHTML='<p class=empty>Loading&hellip;</p>';
    const r=await fetch(API+'/doc?path='+encodeURIComponent(path));
    docCache[path]=r.ok?await r.text():null;
  }
  pane.innerHTML=docCache[path]==null?'<p class=warn>Could not read '+esc(path)+'</p>':md(docCache[path]);
  drawDocBar();
}

// model.rs's score: (impact + urgency + risk) x 10 / effort, to one decimal,
// half to even.
function scoreOf(impact,urgency,risk,effort){
  const tenths=(impact+urgency+risk)*100/Math.max(1,effort), down=Math.floor(tenths);
  const half=Math.abs(tenths-down-0.5)<1e-9;
  return (half?(down%2?down+1:down):Math.round(tenths))/10;
}

// An item's own document (items/<stem>.<type>.md) is edited in its pane;
// the history, and a document of another item, are not.
const docType=path=>path?.startsWith('items/'+IT.stem+'.')?path.slice(('items/'+IT.stem+'.').length,-'.md'.length):null;
function drawDocBar(){
  const own=docType(DOC)!==null&&docCache[DOC]!==null;
  $('#docedit').classList.toggle('hide',!own||DOCEDIT);
  $('#docsave').classList.toggle('hide',!DOCEDIT);
  $('#doccancel').classList.toggle('hide',!DOCEDIT);
  $('#doc').classList.toggle('hide',DOCEDIT);
  $('#doceditor').classList.toggle('hide',!DOCEDIT);
  alignBars();
}
function docEditing(on){
  DOCEDIT=on;
  if(on)$('#doceditor').value=docCache[DOC];
  drawDocBar();
  // focused at the start, so it opens on the document's top, not its end
  if(on){const ed=$('#doceditor');fitEditor(ed);ed.focus();ed.setSelectionRange(0,0);ed.parentElement.scrollTop=0;}
}
$('#docedit').onclick=()=>docEditing(true);
$('#doccancel').onclick=()=>docEditing(false);
$('#docsave').onclick=async()=>{
  const text=$('#doceditor').value;
  try{await api('/doc',{id:IT.id,type:docType(DOC),text});}catch(e){alert(e.message);return;}
  docCache[DOC]=text;
  docEditing(false);
  showDoc(DOC);
};

// An editor is as tall as its text, and at least as tall as the room left
// in its side (or, stacked, in the window), so it never scrolls itself:
// its side does.
function fitEditor(ed){
  if(ed.classList.contains('hide'))return;
  const side=ed.parentElement, css=getComputedStyle(side);
  const scrolls=css.overflowY!=='visible';
  const bottom=scrolls?side.getBoundingClientRect().bottom-parseFloat(css.paddingBottom):innerHeight-16;
  const keep=side.scrollTop;
  ed.style.height='auto';
  const border=ed.offsetHeight-ed.clientHeight;
  ed.style.height=Math.max(ed.scrollHeight+border,bottom-ed.getBoundingClientRect().top)+'px';
  side.scrollTop=keep;
}
const fitEditors=()=>['#editor','#doceditor'].forEach(s=>fitEditor($(s)));
['#editor','#doceditor'].forEach(s=>$(s).addEventListener('input',()=>fitEditor($(s))));
window.addEventListener('resize',fitEditors);

function editing(on){
  EDITING=on;
  $('#editor').value=IT.body;
  drawFields();
  fitEditor($('#editor'));
  ($('#title-field')??$('#controls [data-f=status]'))?.focus();
}
$('#edit').onclick=()=>editing(true);
$('#cancel').onclick=()=>editing(false);
// Save sends what changed in one update, then the kind, which moves the
// item to another lane (its own call, as it renames the item's files).
$('#save').onclick=async()=>{
  const changed={};
  document.querySelectorAll('#controls [data-f]').forEach(el=>{
    if(el.dataset.f==='needs'){
      // sent as typed when the ids differ; the server reads and checks them
      const ids=el.value.split(',').map(s=>s.trim().replace(/^#/,'')).filter(Boolean);
      if(ids.join()!==IT.needs.join())changed.needs=el.value;
      return;
    }
    const v=el.tagName==='SELECT'?el.value:parseInt(el.value,10);
    if(v!==IT[el.dataset.f])changed[el.dataset.f]=v;
  });
  const title=$('#title-field');
  if(title&&title.value.trim()!==IT.title)changed.title=title.value.trim();
  const editor=$('#editor');
  if(!editor.classList.contains('hide')&&editor.value!==IT.body)changed.body=editor.value;
  const kind=$('#kind').classList.contains('hide')||$('#kind').value===IT.kind?null:$('#kind').value;
  try{
    if(Object.keys(changed).length)await api('/items',{id:IT.id,...changed});
    if(kind){
      const r=await api('/items/kind',{id:IT.id,kind});
      location.href=r.stem+location.hash.replace('items%2F'+IT.stem+'.','items%2F'+r.stem+'.');
      return;
    }
  }catch(e){alert(e.message);return;}
  EDITING=false;
  await load();
  $('#msg').textContent='saved';
  setTimeout(()=>$('#msg').textContent='',1500);
};
$('#del').onclick=async()=>{
  const own=IT.docs.filter(d=>d.path.startsWith('items/'+IT.stem+'.')).length;
  const what=own?` and its ${own} document${own>1?'s':''}`:'';
  if(!confirm('Delete '+label(IT.id)+what+'? This cannot be undone.'))return;
  try{await api('/delete',{id:IT.id});}catch(e){alert(e.message);return;}
  location.href='../';
};
window.addEventListener('hashchange',()=>{if(IT)drawDocs();});
load();

// The divider between the text and the documents drags the split. Its place
// is kept per project as the left side's share of the page, so it scales
// with the window; a double-click returns to the default.
const SPLIT='primum-item:'+PROJECT, SIDE_MIN=240;
let leftShare=null;
try{const v=JSON.parse(localStorage.getItem(SPLIT)||'{}').left;if(typeof v==='number')leftShare=v;}catch(e){}
function placeSplit(){
  const split=$('#split');
  if(leftShare===null){split.style.removeProperty('--left');return;}
  const most=split.clientWidth-$('#divider').offsetWidth-2*parseFloat(getComputedStyle(split).columnGap)-SIDE_MIN;
  split.style.setProperty('--left',Math.max(SIDE_MIN,Math.min(most,leftShare*split.clientWidth))+'px');
}
function rememberSplit(){
  try{
    if(leftShare===null)localStorage.removeItem(SPLIT);
    else localStorage.setItem(SPLIT,JSON.stringify({left:leftShare}));
  }catch(e){}
}
const divider=$('#divider');
divider.onpointerdown=e=>{
  e.preventDefault();
  divider.setPointerCapture(e.pointerId);
  const x=e.clientX, start=$('section.left').getBoundingClientRect().width;
  divider.onpointermove=m=>{leftShare=(start+m.clientX-x)/$('#split').clientWidth;placeSplit();};
  divider.onpointerup=()=>{
    divider.onpointermove=null; divider.onpointerup=null;
    // keep what the page shows, not where the pointer went past a side's minimum
    if(leftShare!==null)leftShare=$('section.left').getBoundingClientRect().width/$('#split').clientWidth;
    rememberSplit();
  };
};
divider.ondblclick=()=>{leftShare=null;placeSplit();rememberSplit();};
window.addEventListener('resize',placeSplit);
placeSplit();
// Others' changes arrive by sync; reload every minute unless editing.
setInterval(()=>{
  if(document.visibilityState==='visible'&&!EDITING&&!DOCEDIT)load();
},60000);
