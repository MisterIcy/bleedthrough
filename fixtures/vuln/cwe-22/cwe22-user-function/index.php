<?php

function handle(string $value): void
{
    readfile('/var/www/files/' . $value);
}

handle($_GET['x']);
