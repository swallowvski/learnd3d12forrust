struct Output {
    float4 svpos:SV_POSITION;//システム用頂点座標
    float2 uv:TEXCOORD;//UV値
};

cbuffer cbuff0: register(b0) {
    matrix mat;
}


Output BasicVS(float4 pos: POSITION, float2 uv: TEXCOORD) 
{
    Output output;
    output.svpos = mul(mat, pos);
    output.uv = uv;
    return output;
}