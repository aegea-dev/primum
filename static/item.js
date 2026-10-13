// One item's page: its text and controls on the left, its documents as tabs
// on the right. board.py's expanded row, as a page of its own; `#<path>`
// selects a document's tab.
const $=s=>document.querySelector(s);
let IT=null;
const docCache={};

async function load(){
  const items=await api('/items');
  IT=items.find(i=>i.id===ITEM_ID);
  if(!IT){location.href='../';return;}
  draw();
}

function draw(){
  const closed=isClosed(IT);
  $('#item-id').className='id '+kindClass(IT.kind);
  $('#item-title').textContent=IT.title;
  document.title=IT.id+' '+IT.title+' · primum';
  const score=f=>`<label>${f} <input class=num type=number min=1 max=10 value="${IT[f]}" data-f=${f} ${closed?'disabled':''}></label>`;
  $('#controls').innerHTML=
    `<label>status <select data-f=status>${STATUSES.map(s=>`<option ${s===IT.status?'selected':''}>${s}</option>`).join('')}</select></label>`
    +['impact','urgency','risk','effort'].map(score).join('')
    +`<span>score <span class=score>${IT.score}</span></span>`
    +(IT.needs.length?`<span>needs ${IT.blockers.length?'<span class=warn>'+IT.blockers.join(', ')+'</span>':IT.needs.join(', ')}</span>`:'')
    +(IT.finished?`<span>finished ${esc(IT.finished)}</span>`:'');
  document.querySelectorAll('#controls [data-f]').forEach(el=>el.onchange=async()=>{
    const v=el.tagName==='SELECT'?el.value:parseInt(el.value,10);
    try{await api('/items',{id:IT.id,[el.dataset.f]:v});}catch(e){alert(e.message);}
    await load();
  });
  $('#item-meta').innerHTML=`<span class=docpath>${esc(IT.file)}</span>`;
  $('#view').innerHTML=IT.body.trim()?md(IT.body):'<p class=empty>(no text yet)</p>';
  $('#edit').classList.toggle('hide',closed);
  $('#kind').classList.toggle('hide',closed);
  $('#kindsave').classList.toggle('hide',closed);
  $('#kind').innerHTML=KINDS.map(k=>`<option ${k===IT.kind?'selected':''}>${k}</option>`).join('');
  drawDocs();
}

function drawDocs(){
  const docs=IT.docs;
  $('#split').classList.toggle('single',!docs.length);
  $('#docpane').classList.toggle('hide',!docs.length);
  if(!docs.length)return;
  const label=d=>d.kind+(d.path.startsWith('items/'+IT.stem+'.')?'':': '+d.name.replace(/[.]md$/,''));
  $('#doctabs').innerHTML=docs.map(d=>`<button type=button data-doc="${esc(d.path)}" title="${esc(d.name)}">${esc(label(d))}</button>`).join('');
  document.querySelectorAll('#doctabs button').forEach(b=>b.onclick=()=>{
    history.replaceState(null,'','#'+encodeURIComponent(b.dataset.doc));
    showDoc(b.dataset.doc);
  });
  const want=decodeURIComponent(location.hash.slice(1));
  showDoc(docs.some(d=>d.path===want)?want:docs[0].path);
}

async function showDoc(path){
  document.querySelectorAll('#doctabs button').forEach(b=>b.classList.toggle('on',b.dataset.doc===path));
  $('#docpath').textContent=path;
  const pane=$('#doc');
  if(!(path in docCache)){
    pane.innerHTML='<p class=empty>Loading&hellip;</p>';
    const r=await fetch(API+'/doc?path='+encodeURIComponent(path));
    docCache[path]=r.ok?await r.text():null;
  }
  pane.innerHTML=docCache[path]==null?'<p class=warn>Could not read '+esc(path)+'</p>':md(docCache[path]);
}

function editing(on){
  $('#view').classList.toggle('hide',on);
  $('#editor').classList.toggle('hide',!on);
  $('#save').classList.toggle('hide',!on);
  $('#cancel').classList.toggle('hide',!on);
  $('#edit').classList.toggle('hide',on);
}
$('#edit').onclick=()=>{$('#editor').value=IT.body;editing(true);$('#editor').focus();};
$('#cancel').onclick=()=>editing(false);
$('#save').onclick=async()=>{
  try{await api('/items',{id:IT.id,body:$('#editor').value});}catch(e){alert(e.message);return;}
  editing(false);
  await load();
  $('#msg').textContent='saved';
  setTimeout(()=>$('#msg').textContent='',1500);
};
$('#kindsave').onclick=async()=>{
  const k=$('#kind').value;
  if(k===IT.kind)return;
  try{
    const r=await api('/items/kind',{id:IT.id,kind:k});
    location.href=r.stem+location.hash.replace('items%2F'+IT.stem+'.','items%2F'+r.stem+'.');
  }catch(e){alert(e.message);}
};
$('#del').onclick=async()=>{
  const own=IT.docs.filter(d=>d.path.startsWith('items/'+IT.stem+'.')).length;
  const what=own?` and its ${own} document${own>1?'s':''}`:'';
  if(!confirm('Delete '+IT.id+what+'? This cannot be undone.'))return;
  try{await api('/delete',{id:IT.id});}catch(e){alert(e.message);return;}
  location.href='../';
};
window.addEventListener('hashchange',()=>{if(IT&&IT.docs.length)drawDocs();});
load();
