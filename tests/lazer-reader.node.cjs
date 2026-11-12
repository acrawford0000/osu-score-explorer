const {test}=require('node:test');const assert=require('node:assert/strict');
const {normalizeStatistics}=require('../runtime/statistics.cjs');
test('old standard Good and modern standard ok both retain the 100 count',()=>{
  assert.equal(normalizeStatistics({Great:392,Good:23,Miss:4},0).Ok,23);
  assert.equal(normalizeStatistics({great:392,ok:23,good:0,miss:4},0).Ok,23);
});
test('mania Good remains a 200 judgment, independently of Ok',()=>{
  const s=normalizeStatistics({perfect:400,great:100,good:12,ok:3,meh:1,miss:2},3);
  assert.deepEqual(s,{Perfect:400,Great:100,Good:12,Ok:3,Meh:1,Miss:2});
});
test('catch and standard slider fields preserve their exact counters',()=>{
  const s=normalizeStatistics({large_tick_hit:100,large_tick_miss:2,small_tick_hit:90,small_tick_miss:10,slider_tail_hit:20},2);
  assert.equal(s.LargeTickMiss,2);assert.equal(s.SmallTickMiss,10);assert.equal(s.SliderTailHit,20);
});
