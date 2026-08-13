interface Props {
    width?: number;
    className?: string;
    alt?: string;
}

/** Mimi's square photo mark, at the size needed by its current placement. */
export default function MimiDog({
    width = 40,
    className,
    alt = "Mimi the dog",
}: Props) {
    return (
        <img
            className={["mimi-logo", className].filter(Boolean).join(" ")}
            src="/mimi.jpg"
            width={width}
            height={width}
            alt={alt}
        />
    );
}
