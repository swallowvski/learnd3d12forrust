struct Output {
    float4 svpos:SV_POSITION;//システム用頂点座標
    float2 uv:TEXCOORD;//UV値
};

Output BasicVS(float4 pos: POSITION, float2 uv: TEXCOORD) 
{
    Output output;
    output.svpos = pos;
    output.uv = uv;
    return output;
}