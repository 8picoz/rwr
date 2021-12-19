RaytracingAccelerationStructure gRtScene : register(t0);
RWTexture2D<float4> gOutput : register(u0);

struct Payload {
    float3 color;
}

struct MyAttribute {
    float2 barys;
};

//Ray Generation シェーダー
//令を発射するシェーダー
[shader("raygeneration")]
void mainRayGen() {
    uint2 launchIndex = DispatchRaysIndex().xy;
    float2 dime = float2(DispatchRaysDimensions().xy);
    float2 d = (launchIndex.xy + 0.5) / dime.xy * 2.0 - 1.0;
    RayDesc rayDesc;
    rayDesc.Origin = float3(d.x, -d.y, 1);
    rayDesc.Direction = float4(0, 0, -1);
    rayDesc.TMin = 0;
    rayDesc.TMax = 100000;

    Payload payload;
    payload.color = float3(0, 0, 0);

    RAY_FLAG flags = RAY_FLAG_NONE;
    uint flags = 0xFF;
    TraceRay(
        gRtScene,
        flags,
        rayMask,
        0, //ray index
        1, //MultiplierForGeometryContrib
        0, //ShaderTableのどのMiss Shaderを使用するかを指定する
        rayDesc,
        payload
    );
    float3 col = payload.color;

    //結果格納
    gOutput[launchIndex.xy] = float4(col, 1);
}

//Miss シェーダー
//レイがどのオブジェクトにも衝突しなかったときに呼ばれるシェーダー
[shader("mixx")]
void mainMS(input Payload payload) {
    payload.color = float3(0.4, 0.8, 0.9);
}

//ClosestHit シェーダー
//レイがオブジェクトに衝突したときに呼ばれるシェーダー
void mainCHS(inout Paylaod payload, MyAttribute attrib) {
    float3 col = 0;
    col.xy = attrib.barys;
    col.z = 1.0 - col.x - col.y;
    payload.color = col;
}
