import { useRef, useMemo } from 'react'
import { useFrame, useLoader } from '@react-three/fiber'
import * as THREE from 'three'

const createWaveShader = (texture) => {
  return new THREE.ShaderMaterial({
    uniforms: {
      time: { value: 0 },
      color: { value: new THREE.Color(0.0, 0.5, 1.0) },
      uTexture: { value: texture }
    },
    vertexShader: `
      uniform float time;
      varying float vZ;
      varying vec2 vUv;
      void main() {
        vUv = uv;
        vec3 pos = position;
        // Волны
        float d = sin(pos.x * 0.2 + time) * cos(pos.y * 0.2 + time);
        pos.z += d * 3.0;
        vZ = d;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(pos, 1.0);
      }
    `,
    fragmentShader: `
      uniform vec3 color;
      uniform sampler2D uTexture;
      varying float vZ;
      varying vec2 vUv;
      void main() {
        // Текстура воды + цвет
        vec4 texColor = texture2D(uTexture, vUv + vZ * 0.1); 
        vec3 finalColor = mix(color, texColor.rgb, 0.4);
        finalColor = mix(finalColor, vec3(1.0, 1.0, 1.0), vZ * 0.5);
        
        // --- ИСПРАВЛЕНИЕ КВАДРАТА ---
        // Вычисляем дистанцию от центра (0.5, 0.5)
        float dist = distance(vUv, vec2(0.5));
        // Создаем маску: 1.0 в центре, 0.0 по краям
        float alpha = 1.0 - smoothstep(0.3, 0.5, dist);
        
        // Применяем маску к прозрачности
        gl_FragColor = vec4(finalColor, (0.3 + vZ * 0.3) * alpha);
      }
    `,
    transparent: true,
    wireframe: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false
  })
}

export default function TopologyVisualization() {
  const meshRef = useRef();

  // Загружаем текстуру
  const waterTexture = useLoader(THREE.TextureLoader, 'https://raw.githubusercontent.com/mrdoob/three.js/master/examples/textures/waternormals.jpg');
  waterTexture.wrapS = waterTexture.wrapT = THREE.RepeatWrapping;

  const shaderMaterial = useMemo(() => createWaveShader(waterTexture), [waterTexture]);

  useFrame((state) => {
    shaderMaterial.uniforms.time.value = state.clock.elapsedTime * 0.5;
    if (meshRef.current) {
      meshRef.current.rotation.z = state.clock.elapsedTime * 0.05;
    }
  });

  return (
      <group rotation={[-Math.PI / 3, 0, 0]} position={[0, -5, 0]}>
        {/* Плоскость теперь плавно исчезает к краям */}
        <mesh ref={meshRef} material={shaderMaterial}>
          <planeGeometry args={[100, 100, 50, 50]} />
        </mesh>

        <points>
          <bufferGeometry>
            <bufferAttribute
                attach="attributes-position"
                count={2000}
                array={new Float32Array(6000).map(() => (Math.random() - 0.5) * 150)}
                itemSize={3}
            />
          </bufferGeometry>
          <pointsMaterial size={0.05} color="#00ffff" transparent opacity={0.3} />
        </points>
      </group>
  )
}