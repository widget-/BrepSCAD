// recursive pure version
function sum(v, _i=0) =
    _i >= 500 ? 0 :
    len(v) == 0 ? 0 :
    len(v) == 1 ? v[0] :
    v[0] + sum([ for (j=[1:len(v)-1]) v[j] ], _i + 1);

function reverse(v) =
    len(v) == 0 ? [] :
    [ for (i=[len(v)-1 : -1 : 0]) v[i] ];