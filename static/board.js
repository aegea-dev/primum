// The board: lanes as filters, the priority-ordered table with editable
// scores and status, "next up", and a new item. board.py's page script; a row
// or a docs badge opens the item's own page instead of expanding in place.
let ITEMS=[], kind='ALL', hideDone=true, sort='priority', dir=-1, widths={};
let selectedId=null, focusedField=null, focusAfterDraw=false;
const $=s=>document.querySelector(s);

// The lane, sort, "hide done" and dragged column widths are remembered per
// project.
const PREFS='primum-board:'+PROJECT;
try{
  const p=JSON.parse(localStorage.getItem(PREFS)||'{}');
  if(p.kind)kind=p.kind;
  if(typeof p.hideDone==='boolean')hideDone=p.hideDone;
  if(p.sort){sort=p.sort;dir=p.dir||-1;}
  if(p.widths&&typeof p.widths==='object')widths=p.widths;
}catch(e){}
function remember(){try{localStorage.setItem(PREFS,JSON.stringify({kind,hideDone,sort,dir,widths}));}catch(e){}}

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
  return ITEMS.filter(i=>i.status==='idle'&&!i.blocked).sort(itemOrder)[0];
}
function compareIds(a,b){
  return (a.num-b.num)*dir;
}
function compareItems(a,b){
  if(sort==='priority')return itemOrder(a,b);
  if(sort==='id')return compareIds(a,b);
  const x=sort==='needs'?a.needs.join(','):a[sort], y=sort==='needs'?b.needs.join(','):b[sort];
  return (typeof x==='number'? x-y : String(x).localeCompare(String(y)))*dir;
}

function drawFilters(){
  // what the table can show: closed items count only while they are shown
  const shown=hideDone?ITEMS.filter(i=>!isClosed(i)):ITEMS;
  const counts={}; shown.forEach(i=>counts[i.kind]=(counts[i.kind]||0)+1);
  const open=ITEMS.filter(i=>!isClosed(i)).length;
  const doing=ITEMS.filter(i=>i.status==='doing').length;
  if(kind!=='ALL'&&!counts[kind])kind='ALL';
  $('#filters').innerHTML=
    ['ALL',...KINDS.filter(k=>counts[k])].map(k=>
      `<button type=button class="chip ${k===kind?'on':''} ${k==='ALL'?'':kindClass(k)}" data-k="${k}">${k} ${k==='ALL'?shown.length:counts[k]}</button>`).join('')
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
    <td><select data-f=status aria-label=status>${STATUSES.map(s=>
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
  layoutColumns();
}

// Column widths. Most columns fit their content. title, needs first and docs
// start at a share of the row (flex-basis) and split what is left 3:1:1
// (flex-grow), or give it back in proportion to their share when the row is
// too narrow. A column dragged by its header's grip keeps that width; a
// double-click on the grip fits the column to its content, at most the row.
// needs first and docs never get narrower than their content on one line,
// header included, however wide the title is set: they stay readable.
const FLEX={title:{basis:.4,grow:3},needs:{basis:.1,grow:1,oneLine:true},docs:{basis:.1,grow:1,oneLine:true}};
const MIN_WIDTH=40;
const narrowest=(col,fit)=>FLEX[col]?.oneLine?Math.max(MIN_WIDTH,fit):MIN_WIDTH;
const sum=a=>a.reduce((s,x)=>s+x,0);
// Each shown header with its content's width on one line, and the row's.
function measureColumns(){
  const table=$('table.items');
  const ths=Array.from(table.querySelectorAll('thead th')).filter(th=>th.offsetParent);
  ths.forEach(th=>th.style.width='');
  table.style.width='';
  table.classList.add('measuring');
  const fit=ths.map(th=>th.getBoundingClientRect().width);
  table.classList.remove('measuring');
  return {table,ths,fit,row:table.getBoundingClientRect().width};
}
function layoutColumns(){
  const {table,ths,fit,row}=measureColumns();
  const least=ths.map((th,n)=>narrowest(th.dataset.col,fit[n]));
  const w=ths.map((th,n)=>Math.max(least[n],widths[th.dataset.col]||fit[n]));
  const flex=ths.map((th,n)=>n).filter(n=>FLEX[ths[n].dataset.col]&&!widths[ths[n].dataset.col]);
  if(flex.length){
    const basis=flex.map(n=>FLEX[ths[n].dataset.col].basis*row);
    const left=row-sum(w.filter((x,n)=>!flex.includes(n)))-sum(basis);
    const share=left>=0?flex.map(n=>FLEX[ths[n].dataset.col].grow):basis;
    flex.forEach((n,k)=>{w[n]=Math.max(least[n],basis[k]+left*share[k]/sum(share));});
  }
  // wider than the row when what must stay on one line does not fit in it
  if(!flex.length||sum(w)>row)table.style.width=sum(w)+'px';
  ths.forEach((th,n)=>{th.style.width=w[n]+'px';});
}

document.querySelectorAll('table.items thead th').forEach(th=>{
  const grip=document.createElement('span');
  grip.className='grip';
  th.append(grip);
  grip.onclick=e=>e.stopPropagation();
  grip.onpointerdown=e=>{
    e.preventDefault();
    grip.setPointerCapture(e.pointerId);
    const x=e.clientX, start=th.getBoundingClientRect().width;
    const {ths,fit}=measureColumns(), least=narrowest(th.dataset.col,Math.ceil(fit[ths.indexOf(th)]));
    layoutColumns();
    grip.onpointermove=m=>{
      widths[th.dataset.col]=Math.max(least,Math.round(start+m.clientX-x));
      layoutColumns();
    };
    grip.onpointerup=()=>{grip.onpointermove=null;grip.onpointerup=null;remember();};
  };
  grip.ondblclick=e=>{
    e.stopPropagation();
    const {ths,fit,row}=measureColumns();
    widths[th.dataset.col]=Math.max(MIN_WIDTH,Math.ceil(Math.min(row,fit[ths.indexOf(th)])));
    layoutColumns(); remember();
  };
});
window.addEventListener('resize',layoutColumns);

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
