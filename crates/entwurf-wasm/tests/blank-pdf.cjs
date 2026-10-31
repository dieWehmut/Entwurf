const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const api = require('../pkg-node/entwurf_wasm.js');
const call = (c,a={}) => JSON.parse(api.browser_call(c,JSON.stringify(a)));
// Valid two-page PDF: one text content stream, one intentionally empty stream.
const objects = ['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 7 0 R >> >> /Contents 5 0 R >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 6 0 R >>','<< /Length 48 >>\nstream\nBT /F1 12 Tf 50 700 Td (Evidence) Tj ET        \nendstream','<< /Length 0 >>\nstream\n\nendstream','<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
let pdf='%PDF-1.4\n'; const offsets=[0];
objects.forEach((o,i)=>{offsets.push(Buffer.byteLength(pdf));pdf+=`${i+1} 0 obj\n${o}\nendobj\n`;});
const xref=Buffer.byteLength(pdf);pdf+=`xref\n0 8\n0000000000 65535 f \n${offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')}trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
const bytes=Buffer.from(pdf);const sha=crypto.createHash('sha256').update(bytes).digest('hex');
api.browser_reset();call('create_project',{root:'/projects/blank',name:'Blank PDF'});
let snapshot=call('create_task',{title:'Text plus blank page'});const taskId=snapshot.selected_task_id;
api.browser_put_file('/uploads/blank.pdf',bytes);
const extraction={schema_version:1,source_sha256:sha,source_kind:'pdf',page_count:2,pages:[1,2].map(page=>({page,text:page===1?'Evidence':'',engine:'pdfjs',is_ocr:false,region:{left:0,top:0,width:595000,height:842000,unit:'pdf_millipoints'}})),diagnostics:[]};
call('register_extraction',{path:'/uploads/blank.pdf',extraction});
snapshot=call('import_sources',{taskId,paths:['/uploads/blank.pdf']});
assert.equal(snapshot.current_version.ir.unverified.length,0,'successful blank page must not create an unresolvable review');
snapshot=call('preview_current',{taskId});assert.ok(snapshot.preview_pages.length>0,JSON.stringify(snapshot.current_version.validation));
// A real extraction failure remains a blocker even with an empty transcript.
call('register_extraction',{path:'/uploads/blank.pdf',extraction:{...extraction,diagnostics:[{level:'error',code:'pdf_page_failed',message:'Page 2 could not be extracted'}]}});
snapshot=call('reimport_source',{taskId,baseVersionId:snapshot.current_version.id,sourceId:snapshot.current_version.ir.sources[0].id});
snapshot=call('preview_current',{taskId});assert.equal(snapshot.preview_pages.length,0);assert.ok(snapshot.current_version.validation.findings.some(f=>f.code==='incomplete_source'));
console.log('WASM successful blank PDF page accepted; explicit extraction failure blocks PASS');
