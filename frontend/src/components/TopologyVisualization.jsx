import { useRef, useMemo } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'

// Custom shader material for abstract topology with animated distortion
const createTopologyMaterial = () => {
  return new THREE.ShaderMaterial({
    uniforms: {
      time: { value: 0 },
      color: { value: new THREE.Color(1, 1, 1) },
      opacity: { value: 0.9 }
    },
    vertexShader: `
      uniform float time;
      varying vec3 vPosition;
      varying vec3 vNormal;
      varying float vDistortion;
      
      void main() {
        vPosition = position;
        vNormal = normal;
        
        // Complex distortion using multiple sine waves for undulating motion
        vec3 pos = position;
        float distortion = sin(pos.x * 0.5 + time) * cos(pos.y * 0.3 + time * 0.7) * sin(pos.z * 0.4 + time * 0.5);
        distortion += sin(pos.x * 0.8 + time * 1.2) * sin(pos.y * 0.6 + time * 0.9) * 0.3;
        distortion += cos(pos.z * 1.1 + time * 0.6) * 0.2;
        
        pos += normal * distortion * 0.8;
        vDistortion = distortion;
        
        gl_Position = projectionMatrix * modelViewMatrix * vec4(pos, 1.0);
      }
    `,
    fragmentShader: `
      uniform float time;
      uniform vec3 color;
      uniform float opacity;
      varying vec3 vPosition;
      varying vec3 vNormal;
      varying float vDistortion;
      
      void main() {
        // Create wireframe-like effect with ultra-thin lines
        float lineWidth = 0.002;
        float line = abs(vDistortion);
        line = smoothstep(lineWidth, lineWidth * 0.5, line);
        
        // Add subtle ambient glow
        float glow = abs(vDistortion) * 0.4;
        
        vec3 finalColor = color * (line + glow * 0.2);
        gl_FragColor = vec4(finalColor, opacity * (line + glow * 0.3));
      }
    `,
    transparent: true,
    side: THREE.DoubleSide
  })
}

function AbstractTopologyMesh() {
  const meshRef = useRef()
  const materialRef = useRef()

  // Generate complex, stretched geometry with high density
  const geometry = useMemo(() => {
    const width = 35
    const height = 35
    const widthSegments = 180
    const heightSegments = 180
    const geometry = new THREE.PlaneGeometry(width, height, widthSegments, heightSegments)
    
    // Apply vertex displacement for initial topology shape
    const positions = geometry.attributes.position.array
    for (let i = 0; i < positions.length; i += 3) {
      const x = positions[i]
      const y = positions[i + 1]
      
      // Create warped, stretched topology - interstellar data construct
      const distance = Math.sqrt(x * x + y * y)
      const angle = Math.atan2(y, x)
      
      // Multiple layers of distortion for complexity
      const z1 = Math.sin(distance * 0.4) * Math.cos(angle * 4) * 3.5
      const z2 = Math.cos(distance * 0.6 + angle * 3) * 2.8
      const z3 = Math.sin(angle * 6) * Math.cos(distance * 0.25) * 1.8
      const z4 = Math.sin(distance * 0.3 + angle * 5) * 1.2
      
      positions[i + 2] = (z1 + z2 + z3 + z4) * 0.6
    }
    
    geometry.computeVertexNormals()
    return geometry
  }, [])

  // Create wireframe edges for ultra-thin appearance
  const edgesGeometry = useMemo(() => {
    return new THREE.EdgesGeometry(geometry)
  }, [geometry])

  // Create shader material
  const shaderMaterial = useMemo(() => {
    return createTopologyMaterial()
  }, [])

  useFrame((state) => {
    if (meshRef.current) {
      // Slow, complex rotation and undulation - majestic motion
      meshRef.current.rotation.x = Math.sin(state.clock.elapsedTime * 0.08) * 0.15
      meshRef.current.rotation.y = state.clock.elapsedTime * 0.03
      meshRef.current.rotation.z = Math.cos(state.clock.elapsedTime * 0.06) * 0.08
    }
    
    if (shaderMaterial) {
      shaderMaterial.uniforms.time.value = state.clock.elapsedTime * 0.4
    }
  })

  return (
    <group>
      {/* Main mesh with animated shader */}
      <mesh ref={meshRef} geometry={geometry} material={shaderMaterial} />
      
      {/* Wireframe overlay for ultra-thin lines - 4K quality */}
      <lineSegments geometry={edgesGeometry}>
        <lineBasicMaterial 
          color="#ffffff" 
          transparent
          opacity={0.6}
        />
      </lineSegments>
    </group>
  )
}

function TopologyNetwork() {
  const groupRef = useRef()

  useFrame((state) => {
    if (groupRef.current) {
      // Slow, massive rotation
      groupRef.current.rotation.x = Math.sin(state.clock.elapsedTime * 0.04) * 0.08
      groupRef.current.rotation.y = state.clock.elapsedTime * 0.02
    }
  })

  return (
    <group ref={groupRef}>
      {/* Primary topology layer */}
      <AbstractTopologyMesh />
      
      {/* Secondary layers for depth and complexity */}
      <group position={[0, 0, -5]}>
        <AbstractTopologyMesh />
      </group>
      
      <group position={[0, 0, 5]}>
        <AbstractTopologyMesh />
      </group>
      
      {/* Additional ambient wireframe layers for depth */}
      {[...Array(10)].map((_, i) => {
        const z = -8 + i * 1.6
        const scale = 0.9 + i * 0.06
        const opacity = 0.12 - i * 0.009
        return (
          <mesh key={i} position={[0, 0, z]} scale={scale}>
            <planeGeometry args={[40, 40, 100, 100]} />
            <meshBasicMaterial 
              color="#ffffff" 
              wireframe 
              transparent 
              opacity={Math.max(0.01, opacity)}
            />
          </mesh>
        )
      })}
    </group>
  )
}

export default TopologyNetwork
