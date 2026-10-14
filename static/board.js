// The board: lanes as filters, the priority-ordered table with editable
// scores and status, "next up", and a new item. board.py's page script; a row
// or a docs badge opens the item's own page instead of expanding in place.
let ITEMS=[], kind='ALL', hideDone=true, sort='priority', dir=-1;
let selectedId=null, focusedField=null, focusAfterDraw=false;
const $=s=>document.querySelector(s);

// The lane, sort and "hide done" are remembered per project.
const PREFS='primum-board:'+PROJECT;
try{
  const p=JSON.parse(localStorage.getItem(PREFS)||'{}');
  if(p.kind)kind=p.kind;
  if(typeof p.hideDone==='boolean')hideDone=p.hideDone;
  if(p.sort){sort=p.sort;dir=p.dir||-1;}
}catch(e){}
function remember(){try{localStorage.setItem(PREFS,JSON.stringify({kind,hideDone,sort,dir}));}catch(e){}}

async function load(){
  ITEMS=await api('/items');
  knowItems(ITEMS,'item/');
  draw();
}

async function save(id,field,value){
  const it=ITEMS.find(i=>i.id===id); it[field]=value;
  selectedId=id; focusedField=field; focusAfterDraw=true;
  try{await api('/items',{id,[field]:value});}catch(e){alert(e.message);}
  await load();
}

function visible(){
  let v=ITEMS.filter(i=>kind==='ALL'||i.kind===kind);
  if(hideDone) v=v.filter(i=>!isClosed(i));
  return v.sort(compareItems);
}
function itemOrder(a,b){
  const k=a.kind_rank-b.kind_rank;
  if(k)return k;
  const s=(b.score||0)-(a.score||0);
  if(s)return s;
  for(const [field,mul] of [['impact',-1],['urgency',-1],['risk',-1],['effort',1]]){
    const d=((a[field]||0)-(b[field]||0))*mul;
    if(d)return d;
  }
  return (a.num||0)-(b.num||0);
}
function nextItem(){
  // the next ticket waiting in its lifecycle's first state
  return ITEMS.filter(i=>i.status===i.states[0]&&!i.blocked).sort(itemOrder)[0];
}
function compareIds(a,b){
  if(kind==='ALL'){
    const k=a.kind_rank-b.kind_rank;
    if(k)return k;
  }
  return (a.num-b.num)*dir;
}
function compareItems(a,b){
  if(sort==='priority')return itemOrder(a,b);
  if(sort==='id')return compareIds(a,b);
  const x=sort==='needs'?a.needs.join(','):a[sort], y=sort==='needs'?b.needs.join(','):b[sort];
  return (typeof x==='number'? x-y : String(x).localeCompare(String(y)))*dir;
}

function drawFilters(){
  const counts={}; ITEMS.forEach(i=>counts[i.kind]=(counts[i.kind]||0)+1);
  const open=ITEMS.filter(i=>!isClosed(i)).length;
  const doing=ITEMS.filter(i=>i.status==='doing').length;
  if(kind!=='ALL'&&!counts[kind])kind='ALL';
  $('#filters').innerHTML=
    ['ALL',...KINDS.filter(k=>counts[k])].map(k=>
      `<button type=button class="chip ${k===kind?'on':''} ${k==='ALL'?'':kindClass(k)}" data-k="${k}">${k} ${k==='ALL'?ITEMS.length:counts[k]}</button>`).join('')
    +`<span class=counts>${open} open${doing?`<span class=doing-count>${doing} in progress</span>`:''}</span>`;
  document.querySelectorAll('[data-k]').forEach(b=>b.onclick=()=>{kind=b.dataset.k;remember();draw();});
  $('#hide-done').checked=hideDone;
  $('#sort-select').value=sort;
}

// Finish what is started before starting more: showing "next up" while
// items are in flight invites exactly the wrong thing.
function drawNext(){
  const wip=ITEMS.filter(i=>i.status==='doing').sort(itemOrder);
  const c=nextItem();
  const link=i=>`<a href="item/${i.stem}"><b>${label(i.id)}</b></a>`;
  if(wip.length){
    $('#next').innerHTML=`in progress &rarr; `+wip.map(link).join(', ')
      +` &nbsp;<span class=mut>then ${c?label(c.id)+' ('+esc(c.title)+')':'nothing unblocked'}</span>`;
    return;
  }
  $('#next').innerHTML = c
    ? `next up &rarr; ${link(c)} &nbsp;${esc(c.title)} &nbsp;<span class=mut>score ${c.score}</span>`
    : 'nothing unblocked and open.';
}

// Scores on finished work are inert: editing them changes nothing and only
// invites doubt about whether the ordering still means something.
const fin=i=>isClosed(i)?'disabled':'';

function markSelected(follow=false){
  document.querySelectorAll('tr.item.selected').forEach(tr=>tr.classList.remove('selected'));
  if(!selectedId)return;
  const row=Array.from(document.querySelectorAll('tr.item')).find(tr=>tr.dataset.id===selectedId);
  if(!row)return;
  row.classList.add('selected');
  if(follow)row.scrollIntoView({block:'nearest'});
  if(follow&&focusedField){
    const el=row.querySelector(`[data-f="${focusedField}"]`);
    if(el){el.focus();if(el.select)el.select();}
  }
}

function openItem(stem,doc){
  location.href='item/'+stem+(doc?'#'+encodeURIComponent(doc):'');
}

function draw(){
  drawFilters(); drawNext();
  document.querySelectorAll('th[data-s]').forEach(th=>th.classList.toggle('sorted',th.dataset.s===sort));
  $('#rows').innerHTML=visible().map((i,n)=>`
   <tr class="item ${n%2?'alt':''} ${i.id===selectedId?'selected':''} ${i.status==='doing'?'doing':''} ${i.blocked?'blocked':''} ${isClosed(i)?'done':''}" data-id="${i.id}" data-stem="${i.stem}">
    <td class="id ${kindClass(i.kind)}" title="${i.kind}">${label(i.id)}</td>
    <td class=title><a href="item/${i.stem}">${esc(i.title)}</a>${i.conflict?'<span class=conflict-mark title="changed on two sides: open to resolve">!</span>':''} ${i.body.trim()?'':'<span class=warn title="no text yet">&#9888;</span>'}</td>
    <td class=n><input class=num type=number min=1 max=10 value="${i.impact}" data-f=impact aria-label=impact ${fin(i)}></td>
    <td class=n><input class=num type=number min=1 max=10 value="${i.urgency}" data-f=urgency aria-label=urgency ${fin(i)}></td>
    <td class=n><input class=num type=number min=1 max=10 value="${i.risk}" data-f=risk aria-label=risk ${fin(i)}></td>
    <td class=n><input class=num type=number min=1 max=10 value="${i.effort}" data-f=effort aria-label=effort ${fin(i)}></td>
    <td class="n score">${i.score}</td>
    <td>${i.blockers.length?'<span class=warn>'+i.blockers.map(label).join(', ')+'</span>':(i.needs.map(label).join(', ')||'-')}</td>
    <td class=docs>${i.docs.map(d=>`<button type=button class=docbadge data-doc="${esc(d.path)}" title="${esc(d.name)}">${esc(d.kind)}</button>`).join('')}</td>
    <td><select data-f=status aria-label=status>${i.states.map(s=>
        `<option ${s===i.status?'selected':''}>${s}</option>`).join('')}</select></td>
   </tr>`).join('');

  document.querySelectorAll('#rows [data-f]').forEach(el=>{
    el.onclick=e=>e.stopPropagation();
    el.onfocus=()=>{selectedId=el.closest('tr').dataset.id;focusedField=el.dataset.f;markSelected();};
    el.onchange=()=>{const id=el.closest('tr').dataset.id;
      const v=el.tagName==='SELECT'?el.value:parseInt(el.value,10); save(id,el.dataset.f,v);};
  });
  document.querySelectorAll('tr.item').forEach(tr=>tr.onclick=e=>{
    if(e.target.closest('a'))return;
    openItem(tr.dataset.stem);
  });
  document.querySelectorAll('#rows .docbadge').forEach(b=>b.onclick=e=>{
    e.stopPropagation(); openItem(b.closest('tr').dataset.stem,b.dataset.doc);});
  markSelected(focusAfterDraw); focusAfterDraw=false;
}

document.querySelectorAll('th[data-s]').forEach(th=>th.onclick=()=>{
  const s=th.dataset.s;
  dir=(s===sort)? -dir : (['impact','urgency','risk','score'].includes(s)?-1:1);
  sort=s; remember(); draw();
});
$('#sort-select').onchange=e=>{
  sort=e.target.value; dir=['impact','urgency','risk','score'].includes(sort)?-1:1; remember(); draw();
};
$('#hide-done').onchange=e=>{hideDone=e.target.checked;remember();draw();};

// Up and down (or j and k) move the selection, Enter opens it.
document.addEventListener('keydown',e=>{
  if(e.target.closest('input,select,textarea')||e.metaKey||e.ctrlKey||e.altKey)return;
  const rows=Array.from(document.querySelectorAll('tr.item'));
  if(!rows.length)return;
  let at=rows.findIndex(tr=>tr.dataset.id===selectedId);
  if(e.key==='ArrowDown'||e.key==='j')at=Math.min(rows.length-1,at+1);
  else if(e.key==='ArrowUp'||e.key==='k')at=Math.max(0,at-1);
  else if(e.key==='Enter'&&at>=0){openItem(rows[at].dataset.stem);return;}
  else return;
  e.preventDefault();
  selectedId=rows[at].dataset.id; focusedField=null; markSelected(true);
});

$('#nnew').onclick=()=>{$('#newform').classList.toggle('on');if($('#newform').classList.contains('on'))$('#ntitle').focus();};
$('#ncancel').onclick=()=>$('#newform').classList.remove('on');
$('#newform').onsubmit=async e=>{
  e.preventDefault();
  const title=$('#ntitle').value.trim(); if(!title)return;
  try{
    const r=await api('/new',{kind:$('#nkind').value,title,
      impact:+$('#nimpact').value,urgency:+$('#nurgency').value,
      risk:+$('#nrisk').value,effort:+$('#neffort').value});
    selectedId=r.id; focusAfterDraw=true; focusedField=null;
  }catch(err){alert(err.message);return;}
  $('#ntitle').value=''; $('#newform').classList.remove('on'); await load();
};
load();
// Others' changes arrive by sync; reload every minute while the page is
// visible and nothing is being edited.
setInterval(()=>{
  if(document.visibilityState==='visible'&&!document.activeElement.closest('input,select,textarea'))load();
},60000);
