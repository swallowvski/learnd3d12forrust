struct Output
{
    float4 svpos:SV_POSITION;
    float2 uv:TEXCOORD;
};

Texture2D<float4> tex:register(t0);
SamplerState smp:register(s0);

float4
BasicPS(Output input) : SV_TARGET
{
    return float4(tex.Sample(smp, input.uv));
}